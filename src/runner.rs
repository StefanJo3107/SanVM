use std::{env, io};
use std::fs::File;
use std::io::Read;
use std::process::exit;
use postcard::{Error, from_bytes, to_allocvec};
use sanscript_common::value::{FunctionData, FunctionType};
use crate::{DebugLevel, VM};

pub fn run() {
    if env::args().len() == 2 {
        let args: Vec<String> = env::args().collect();
        if let Err(e) = run_file(args[1].as_str()) {
            eprintln!("{}", e.to_string());
            exit(1);
        }
    } else {
        eprintln!("Usage: SanScript [path]");
        exit(1);
    }
}

pub fn run_file(bytecode_path: &str) -> io::Result<()> {
    read_file(bytecode_path);
    Ok(())
}

fn read_file(bytecode_path: &str) {
    let mut bytecode_file = File::open(bytecode_path).expect("Cannot open file at path");
    let mut bytecode: Vec<u8> = vec![];
    bytecode_file.read_to_end(&mut bytecode).expect("Cannot read contents of a file");
    let out: Result<FunctionData, Error> = from_bytes(&bytecode.as_slice());
    let mut vm = VM::new(DebugLevel::None);
    vm.interpret(out);
}

pub fn deserialize_bytecode(bytecode: &[u8]) {
    let out: Result<FunctionData, Error> = from_bytes(&bytecode);
    let mut vm = VM::new(DebugLevel::None);
    vm.interpret(out);
}