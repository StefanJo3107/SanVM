use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use sanscript_common::chunk::OpCode;
use sanscript_common::debug::disassemble_instruction;
use sanscript_common::value::{FunctionData, FunctionType, Value, ValueArray};
use sanscript_frontend::compiler::Compiler;
use sanscript_frontend::scanner::Scanner;
use crate::InterpretResult::{InterpretCompileError, InterpretOK, InterpretRuntimeError};

pub enum InterpretResult {
    InterpretOK,
    InterpretCompileError,
    InterpretRuntimeError,
}

#[derive(PartialEq, Copy, Clone)]
pub enum DebugLevel {
    None,
    TokenTableOnly,
    BytecodeOnly,
    Verbose,
}

type FrameRef = Rc<RefCell<Vec<CallFrame>>>;

const STACK_SIZE: usize = 256;
const MAX_CALL_FRAME_DEPTH: usize = 64;

struct CallFrame {
    function: FunctionData,
    ip: usize,
    stack_start: usize,
}

pub struct VM {
    frames: FrameRef,
    stack: Vec<Value>,
    globals: HashMap<String, Value>,
    debug_level: DebugLevel,
}

impl VM {
    pub fn new(debug_level: DebugLevel) -> VM {
        VM {
            frames: Rc::new(RefCell::new(vec![])),
            stack: vec![],
            globals: HashMap::new(),
            debug_level,
        }
    }

    // pub fn interpret(&mut self, name: &str) -> InterpretResult {
    //     self.run(name)
    // }

    pub fn interpret(&mut self, source: String) -> InterpretResult {
        if self.debug_level == DebugLevel::Verbose || self.debug_level == DebugLevel::TokenTableOnly {
            let mut scanner = Scanner::new(source.as_str());
            scanner.tokenize_source();
        }

        let mut compiler = Compiler::new(source.as_str(), FunctionType::Script);

        if let Some(function) = compiler.compile() {
            self.stack.push(Value::ValFunction(function.clone()));
            self.frames.borrow_mut().push(CallFrame { function: function.clone(), ip: 0, stack_start: self.stack.len() - 1 });
            let mut frames_cloned = self.frames.clone();
            let mut frames_borrowed = frames_cloned.borrow_mut();
            let mut frame = frames_borrowed.last_mut().unwrap_or_else(||{panic!("Call frame vector is empty!")});
            self.call(function, frame, 0);
            //should drop frames_borrowed because frames get borrowed mutably inside self.run
            drop(frames_borrowed);
            let result = self.run();
            return result;
        }

        return InterpretCompileError;
    }

    fn is_number_operands(&self) -> bool {
        return matches!(self.stack.last().unwrap_or_else(|| {panic!("Error reading last element of the stack!")}), Value::ValNumber(_)) && matches!(self.stack.get(self.stack.len() - 2).unwrap_or_else(||{panic!("Error reading second to last element of the stack!");}), Value::ValNumber(_));
    }

    fn is_string_operands(&self) -> bool {
        return matches!(self.stack.last().unwrap_or_else(|| {panic!("Error reading last element of the stack!")}), Value::ValString(_)) && matches!(self.stack.get(self.stack.len() - 2).unwrap_or_else(||{panic!("Error reading second to last element of the stack!");}), Value::ValString(_));
    }

    //most important function so far
    fn run(&mut self) -> InterpretResult {
        macro_rules! binary_op {
            (Value::ValString, +) => {
                if let Value::ValString(b) = self.stack.pop().unwrap() {
                    if let Value::ValString(a) = self.stack.pop().unwrap() {
                        self.stack.push(Value::ValString(format!("{}{}", a, b)));
                    }
                }
            };
            ($value_type: path,$op: tt) => {
                if !self.is_number_operands() {
                    self.runtime_error("Operands must be numbers.");
                    return InterpretRuntimeError;
                }
                if let Value::ValNumber(b) = self.stack.pop().unwrap() {
                    if let Value::ValNumber(a) = self.stack.pop().unwrap() {
                        self.stack.push($value_type(a $op b));
                    }
                }
            }
        }

        if self.debug_level == DebugLevel::Verbose || self.debug_level == DebugLevel::BytecodeOnly {
            //printing disassembler header
            println!("\x1B[4mOFFSET |  LINE  | {: <30}\x1B[0m", "OPCODE");
        }

        let mut print_offsets: Vec<usize> = vec![];
        print_offsets.push(0);
        let mut print_ip = self.frames.borrow_mut().last().unwrap().ip;

        loop {
            let frames_rc = self.frames.clone();
            let mut frames_mut = frames_rc.borrow_mut();
            let mut frame = frames_mut.last_mut().unwrap();
            let chunk = &frame.function.chunk;
            let instruction: &OpCode = chunk.get_code(frame.ip);

            if self.debug_level == DebugLevel::Verbose || self.debug_level == DebugLevel::BytecodeOnly {
                for ip in print_ip..frame.ip + 1 {
                    if frame.ip - ip >= 1 {
                        print!("\x1b[31m{:0>6} |", print_offsets.last().unwrap());
                    } else {
                        print!("\x1b[0m{:0>6} |", print_offsets.last().unwrap());
                    }
                    let off = disassemble_instruction(chunk, ip, &mut print_offsets);
                    print_offsets.push(off);
                    //printing stack
                    for value in self.stack.iter() {
                        if frame.ip - ip >= 1 {
                            print!("\x1b[31m[ ");
                            ValueArray::print_value(value);
                            print!("\x1b[31m ]");
                        } else {
                            print!("\x1b[0m[ ");
                            ValueArray::print_value(value);
                            print!("\x1b[0m ]");
                        }
                    }

                    if self.stack.len() > 0 {
                        println!("\x1b[0m");
                    }
                }
                if print_ip <= frame.ip {
                    print_ip = frame.ip + 1;
                }
            }

            match instruction
            {
                OpCode::OpReturn => {
                    return InterpretOK;
                }
                OpCode::OpPrint => {
                    ValueArray::print_value(&self.stack.pop().unwrap_or_else(|| { Value::ValString(String::from("")) }));
                    println!();
                }
                OpCode::OpPop => {
                    self.stack.pop();
                }
                OpCode::OpConstant(constant_addr) => {
                    let constant = chunk.get_constant(constant_addr.to_owned());
                    self.stack.push(constant.to_owned());
                }
                OpCode::OpDefineGlobal(global_addr) => {
                    let name_value = chunk.get_constant(global_addr.to_owned());
                    if let Value::ValString(name) = name_value {
                        self.globals.insert(name.to_owned(), self.stack.pop().unwrap_or_else(|| { panic!("Stack is empty, cannot define global variable") }));
                    }
                }
                OpCode::OpGetGlobal(global_addr) => {
                    let name_value = chunk.get_constant(global_addr.to_owned());
                    if let Value::ValString(name) = name_value {
                        if let Some(var_value) = self.globals.get(name) {
                            self.stack.push(var_value.to_owned());
                        } else {
                            self.runtime_error(format!("Undefined variable '{}'", name).as_str());
                            return InterpretRuntimeError;
                        }
                    }
                }
                OpCode::OpSetGlobal(global_addr) => {
                    let name_value = chunk.get_constant(global_addr.to_owned());
                    if let Value::ValString(name) = name_value {
                        if let Some(_) = self.globals.get(name) {
                            self.globals.insert(name.to_owned(), self.stack.last().unwrap_or_else(|| { panic!("Stack is empty, cannot define global variable") }).clone());
                        } else {
                            self.runtime_error(format!("Undefined variable '{}'", name).as_str());
                            return InterpretRuntimeError;
                        }
                    }
                }
                OpCode::OpGetLocal(local_addr) => {
                    let stack_start = frame.stack_start;
                    let stack_val = self.stack.get(stack_start + *local_addr).unwrap_or_else(|| { panic!("Stack is empty, cannot get local variable") });
                    self.stack.push(stack_val.clone());
                }
                OpCode::OpSetLocal(local_addr) => {
                    let stack_start = frame.stack_start;
                    self.stack[stack_start + *local_addr] = self.stack.last().unwrap_or_else(|| { panic!("Stack is empty, cannot set local variable") }).clone();
                }
                OpCode::OpNegate => {
                    if let Some(Value::ValNumber(number)) = self.stack.last() {
                        self.stack.push(Value::ValNumber(-*number));
                        //remove element that used to be last
                        self.stack.remove(self.stack.len() - 2);
                    } else {
                        self.runtime_error("Operand must be a number.");
                        return InterpretRuntimeError;
                    }
                }
                OpCode::OpAdd => {
                    if self.is_number_operands() {
                        binary_op!(Value::ValNumber, +);
                    }

                    if self.is_string_operands() {
                        binary_op!(Value::ValString, +);
                    }
                }
                OpCode::OpSubtract => {
                    binary_op!(Value::ValNumber, -);
                }
                OpCode::OpMultiply => {
                    binary_op!(Value::ValNumber, *);
                }
                OpCode::OpDivide => {
                    binary_op!(Value::ValNumber, /);
                }
                OpCode::OpTrue => {
                    self.stack.push(Value::ValBool(true))
                }
                OpCode::OpFalse => {
                    self.stack.push(Value::ValBool(false))
                }
                OpCode::OpNil => {
                    self.stack.push(Value::ValNil)
                }
                OpCode::OpNot => {
                    let value = self.stack.pop().unwrap_or_else(|| { panic!("Stack is empty."); });
                    self.stack.push(Value::ValBool(self.is_falsey(value)));
                }
                OpCode::OpEqual => {
                    let b = self.stack.pop().unwrap_or_else(|| { panic!("Stack is empty."); });
                    let a = self.stack.pop().unwrap_or_else(|| { panic!("Stack is empty."); });
                    self.stack.push(Value::ValBool(self.equals(a, b)));
                }
                OpCode::OpGreater => {
                    binary_op!(Value::ValBool, >);
                }
                OpCode::OpLess => {
                    binary_op!(Value::ValBool, <);
                }
                OpCode::OpJumpIfFalse(offset) => {
                    if self.is_falsey(self.stack.last().unwrap_or_else(|| { panic!("Stack is empty.") }).clone()) {
                        frame.ip += offset;
                    }
                }
                OpCode::OpJumpIfTrue(offset) => {
                    if !self.is_falsey(self.stack.last().unwrap_or_else(|| { panic!("Stack is empty.") }).clone()) {
                        frame.ip += offset;
                    }
                }
                OpCode::OpJump(offset) => {
                    frame.ip += offset;
                }
                OpCode::OpLoop(offset) => {
                    frame.ip -= offset;
                }
                OpCode::OpCall(arg_count) => {
                    let callee = self.stack.get(self.stack.len() - 1 - arg_count).unwrap_or_else(|| { panic!("Couldn't get callee value!") }).clone();
                    if !self.call_value(callee, frame, *arg_count) {
                        return InterpretRuntimeError;
                    }
                    continue;
                }
            };

            frame.ip += 1;
        }
    }

    fn call_value(&mut self, callee: Value, frame: &mut CallFrame, arg_count: usize) -> bool {
        if let Value::ValFunction(func) = callee {
            return self.call(func, frame, arg_count);
        }

        self.runtime_error("Can only call functions");
        false
    }

    fn call(&mut self, func: FunctionData, frame: &mut CallFrame, arg_count: usize) -> bool {
        frame.function = func;
        frame.ip = 0;
        frame.stack_start = self.stack.len() - arg_count - 1;
        true
    }

    fn is_falsey(&self, value: Value) -> bool {
        return match value {
            Value::ValBool(boolean) => !boolean,
            Value::ValNumber(number) => number == 0.0,
            Value::ValNil => true,
            _ => false //TODO
        };
    }

    fn equals(&self, a: Value, b: Value) -> bool {
        return match (a, b) {
            (Value::ValNumber(num_a), Value::ValNumber(num_b)) => num_a == num_b,
            (Value::ValBool(bool_a), Value::ValBool(bool_b)) => bool_a == bool_b,
            (Value::ValNil, Value::ValNil) => true,
            (Value::ValString(string_a), Value::ValString(string_b)) => string_a == string_b,
            _ => false
        };
    }

    pub fn runtime_error(&mut self, message: &str) {
        eprintln!("{}", message);

        eprintln!("[line {}] in script", self.frames.borrow_mut().last().unwrap().function.chunk.get_line(self.frames.borrow_mut().last().unwrap().ip - 1));
        self.stack = vec![];
    }
}