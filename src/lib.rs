pub mod runner;
pub mod actuators;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use postcard::Error;
use san_common::chunk::OpCode;
use san_common::debug::disassemble_instruction;
use san_common::hid_actuator::HidActuator;
use san_common::native_functions::NativeFns;
use san_common::value::{FunctionData, NativeFunctionData, Value, ValueArray};
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

// const STACK_SIZE: usize = 256;
const MAX_CALL_FRAME_DEPTH: usize = 256;

#[derive(Clone)]
pub struct CallFrame {
    function: FunctionData,
    ip: usize,
    print_ip: usize,
    stack_start: usize,
}

pub struct VM<T: HidActuator> {
    frames: FrameRef,
    stack: Vec<Value>,
    globals: HashMap<String, Value>,
    native_functions: NativeFns<T>,
    debug_level: DebugLevel,
}

impl<T: HidActuator> VM<T> {
    pub fn new(hid_actuator: T, debug_level: DebugLevel) -> VM<T> {
        let mut vm = VM {
            frames: Rc::new(RefCell::new(vec![])),
            stack: vec![],
            globals: HashMap::new(),
            native_functions: NativeFns::new(hid_actuator),
            debug_level,
        };

        vm.globals.insert(String::from("sleep"), Value::ValNativeFn(NativeFunctionData{arity: 1, name: String::from("sleep")}));
        vm.globals.insert(String::from("random_int"), Value::ValNativeFn(NativeFunctionData{arity: 2, name: String::from("random_int")}));
        vm.globals.insert(String::from("random_float"), Value::ValNativeFn(NativeFunctionData{arity: 2, name: String::from("random_float")}));
        vm.globals.insert(String::from("inject_keys"), Value::ValNativeFn(NativeFunctionData{arity: 1, name: String::from("inject_keys")}));
        vm.globals.insert(String::from("hold_keys"), Value::ValNativeFn(NativeFunctionData{arity: 1, name: String::from("hold_keys")}));
        vm.globals.insert(String::from("inject_sequence"), Value::ValNativeFn(NativeFunctionData{arity: 3, name: String::from("inject_sequence")}));
        vm.globals.insert(String::from("release_keys"), Value::ValNativeFn(NativeFunctionData{arity: 0, name: String::from("release_keys")}));
        vm.globals.insert(String::from("string_to_keys"), Value::ValNativeFn(NativeFunctionData{arity: 1, name: String::from("string_to_keys")}));
        vm.globals.insert(String::from("type_string"), Value::ValNativeFn(NativeFunctionData{arity: 3, name: String::from("type_string")}));
        vm.globals.insert(String::from("mouse_move"), Value::ValNativeFn(NativeFunctionData{arity: 2, name: String::from("mouse_move")}));
        vm.globals.insert(String::from("mouse_click"), Value::ValNativeFn(NativeFunctionData{arity: 1, name: String::from("mouse_click")}));
        vm.globals.insert(String::from("mouse_hold"), Value::ValNativeFn(NativeFunctionData{arity: 1, name: String::from("mouse_hold")}));
        vm.globals.insert(String::from("mouse_up"), Value::ValNativeFn(NativeFunctionData{arity: 0, name: String::from("mouse_up")}));
        vm
    }

    pub fn interpret(&mut self, bytecode: Result<FunctionData, Error>) -> InterpretResult {

        if let Ok(function) = bytecode {
            self.stack.push(Value::ValFunction(function.clone()));
            self.frames.borrow_mut().push(CallFrame { function: function.clone(), ip: 0, print_ip: 0, stack_start: self.stack.len() - 1 });
            let frames_cloned = self.frames.clone();
            let mut frames_borrowed = frames_cloned.borrow_mut();
            let frame_count = frames_borrowed.len();
            let frame = frames_borrowed.last_mut().unwrap_or_else(|| { panic!("Call frame vector is empty!") });
            self.call(function, frame, frame_count, 0);
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

    fn is_hid_operands(&self) -> bool {
        return matches!(self.stack.last().unwrap_or_else(|| {panic!("Error reading last element of the stack!")}), Value::ValKey(_)) && matches!(self.stack.get(self.stack.len() - 2).unwrap_or_else(||{panic!("Error reading second to last element of the stack!");}), Value::ValKey(_));
    }

    fn is_string_operands(&self) -> bool {
        return matches!(self.stack.last().unwrap_or_else(|| {panic!("Error reading last element of the stack!")}), Value::ValString(_)) && matches!(self.stack.get(self.stack.len() - 2).unwrap_or_else(||{panic!("Error reading second to last element of the stack!");}), Value::ValString(_));
    }

    //most important function so far
    fn run(&mut self) -> InterpretResult {
        macro_rules! binary_op {
            (Value::ValKey, +, $frame: expr) => {
                if let Value::ValKey(mut b) = self.stack.pop().unwrap() {
                    if let Value::ValKey(mut a) = self.stack.pop().unwrap() {
                        if b.len() + a.len() > 6 {
                            self.runtime_error("Only 6 key rollover supported, but tried to press more", $frame);
                            return InterpretRuntimeError;
                        }
                        a.append(&mut b);
                        self.stack.push(Value::ValKey(a));
                    }
                }
            };
            (Value::ValString, +) => {
                if let Value::ValString(b) = self.stack.pop().unwrap() {
                    if let Value::ValString(a) = self.stack.pop().unwrap() {
                        self.stack.push(Value::ValString(format!("{}{}", a, b)));
                    }
                }
            };
            ($value_type: path,$op: tt,$frame: expr) => {
                if !self.is_number_operands() {
                    self.runtime_error("Operands must be numbers.", $frame);
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
        let frames_rc = self.frames.clone();
        let mut frames_mut = frames_rc.borrow_mut();

        loop {
            let frame_count = frames_mut.len();
            let mut frame = frames_mut.last_mut().unwrap();
            let chunk = &frame.function.chunk;
            let instruction: &OpCode = chunk.get_code(frame.ip);

            if self.debug_level == DebugLevel::Verbose || self.debug_level == DebugLevel::BytecodeOnly {
                for ip in frame.print_ip..frame.ip + 1 {
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
                if frame.print_ip <= frame.ip {
                    frame.print_ip = frame.ip + 1;
                }
            }

            match instruction
            {
                OpCode::OpReturn => {
                    let result = self.stack.pop().unwrap_or_else(|| { panic!("Stack is empty, cannot pop from it") });
                    frames_mut.remove(frame_count - 1);
                    if frame_count - 1 == 0 {
                        self.stack.pop().unwrap_or_else(|| { panic!("Stack is empty, cannot pop from it") });
                        return InterpretOK;
                    }
                    while !matches!(self.stack.last().unwrap_or_else(|| { panic!("Stack is empty") }), Value::ValFunction(_)) {
                        self.stack.pop();
                    }
                    self.stack.pop();

                    if self.debug_level == DebugLevel::Verbose || self.debug_level == DebugLevel::BytecodeOnly {
                        println!("\x1B[32;1m---------------------------------------------\x1B[0m");
                        println!();
                    }

                    if frames_mut.len() > 0 {
                        self.stack.push(result);
                        frame = frames_mut.last_mut().unwrap_or_else(|| { panic!("Call frame vector is empty") });
                    } else {
                        return InterpretOK;
                    }
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
                            self.runtime_error(format!("Undefined variable '{}'", name).as_str(), frame);
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
                            self.runtime_error(format!("Undefined variable '{}'", name).as_str(), frame);
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
                        self.runtime_error("Operand must be a number.", frame);
                        return InterpretRuntimeError;
                    }
                }
                OpCode::OpAdd => {
                    if self.is_number_operands() {
                        binary_op!(Value::ValNumber, +, frame);
                    } else if self.is_string_operands() {
                        binary_op!(Value::ValString, +);
                    } else if self.is_hid_operands() {
                        binary_op!(Value::ValKey, +, frame);
                    }
                }
                OpCode::OpSubtract => {
                    binary_op!(Value::ValNumber, -, frame);
                }
                OpCode::OpMultiply => {
                    binary_op!(Value::ValNumber, *, frame);
                }
                OpCode::OpDivide => {
                    binary_op!(Value::ValNumber, /, frame);
                }
                OpCode::OpPipe => {
                    let s1 = self.stack.pop().unwrap();
                    let s2 = self.stack.pop().unwrap();
                    if let Value::ValKey(b) = s1 {
                        if let Value::ValKey(a) = s2 {
                            let sequence = Value::ValKeySequence(vec![a, b]);
                            self.stack.push(sequence);
                        } else if let Value::ValKeySequence(mut a) = s2 {
                            a.push(b);
                            self.stack.push(Value::ValKeySequence(a));
                        }
                    } else if let Value::ValKeySequence(mut b) = s1 {
                        if let Value::ValKey(a) = s2 {
                            b.insert(0, a);
                            self.stack.push(Value::ValKeySequence(b));
                        } else if let Value::ValKeySequence(mut a) = s2 {
                            a.append(&mut b);
                            self.stack.push(Value::ValKeySequence(a));
                        }
                    }
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
                    binary_op!(Value::ValBool, >, frame);
                }
                OpCode::OpLess => {
                    binary_op!(Value::ValBool, <, frame);
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
                    let mut new_frame = frame.clone();
                    if !self.call_value(callee.clone(), &mut new_frame, frame_count, *arg_count) {
                        return InterpretRuntimeError;
                    }

                    if self.debug_level == DebugLevel::Verbose || self.debug_level == DebugLevel::BytecodeOnly {
                        println!();
                        println!("\x1B[32;1m------------------ {}({}) ------------------\x1B[0m", new_frame.function.name, new_frame.function.arity);
                    }

                    if !matches!(callee, Value::ValNativeFn(_)) {
                        frames_mut.push(new_frame);
                        continue;
                    }
                }
            };

            frame.ip += 1;
        }
    }

    fn call_value(&mut self, callee: Value, frame: &mut CallFrame, frame_count: usize, arg_count: usize) -> bool {
        if let Value::ValFunction(func) = callee {
            return self.call(func, frame, frame_count, arg_count);
        } else if let Value::ValNativeFn(func) = callee {
            let native = self.native_functions.get_function_from_data(&func);
            let mut args: Vec<Value> = vec![];
            for _ in 0..func.arity {
                args.push(self.stack.pop().expect("Stack is empty."));
            }
            let result = native(&mut self.native_functions, args);
            self.stack.pop();
            self.stack.push(result);
            return true;
        }

        self.runtime_error("Can only call functions", frame);
        false
    }

    fn call(&mut self, func: FunctionData, frame: &mut CallFrame, frame_count: usize, arg_count: usize) -> bool {
        if func.arity != arg_count {
            self.runtime_error(format!("Expected {} arguments, but got {}", func.arity, arg_count).as_str(), frame);
            return false;
        }

        if frame_count >= MAX_CALL_FRAME_DEPTH {
            self.runtime_error("Stack overflow detected", frame);
            return false;
        }

        frame.function = func;
        frame.ip = 0;
        frame.print_ip = 0;
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

    pub fn runtime_error(&mut self, message: &str, frame: &mut CallFrame) {
        eprintln!("{}", message);
        eprintln!("[line {}] in script", frame.function.chunk.get_line(frame.ip - 1));
        self.stack = vec![];
    }
}