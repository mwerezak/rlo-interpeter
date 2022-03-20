// disable these until we have a working system
#![allow(dead_code)]
#![allow(unused_imports)]

use std::io;

#[macro_use]
extern crate lazy_static;

pub mod utils;

pub mod source;
pub mod lexer;
pub mod parser;

pub mod language;
pub mod codegen;
pub mod runtime;

pub mod frontend;
pub mod debug;


use source::{SourceText, ModuleSource, ParseContext};
use parser::ParserError;
use parser::stmt::StmtMeta;
use codegen::{CompiledProgram, Compiler, UnloadedProgram, CompileError};
use runtime::strings::StringInterner;

#[derive(Debug)]
pub enum BuildErrors {
    // depending on which stage the build failed
    Source(io::Error),
    Syntax(Box<[ParserError]>),
    Compile(Box<[CompileError]>),
}

pub fn build_module(module: &ModuleSource) -> Result<CompiledProgram, BuildErrors> {
    let source_text = module.source_text();
    if source_text.is_err() {
        return Err(BuildErrors::Source(source_text.unwrap_err()));
    }
    
    build_source(source_text.unwrap())
}

pub fn build_source(source_text: SourceText) -> Result<CompiledProgram, BuildErrors> {
    let mut interner = StringInterner::new();
    
    // parsing
    let parse_result = parse_source(&mut interner, source_text);
    if parse_result.is_err() {
        let errors = parse_result.unwrap_err().into_boxed_slice();
        return Err(BuildErrors::Syntax(errors));
    }
    
    // compilation
    let compile_result = compile_ast(interner, parse_result.unwrap());
    
    if compile_result.is_err() {
        let errors = compile_result.unwrap_err().into_boxed_slice();
        return Err(BuildErrors::Compile(errors));
    }
    
    Ok(compile_result.unwrap())
}

/// Produce AST from SourceText
pub fn parse_source(interner: &mut StringInterner, source_text: SourceText) -> Result<Vec<StmtMeta>, Vec<ParserError>> {
    let lexer_factory = language::create_default_lexer_rules();
    let mut parse_ctx = ParseContext::new(&lexer_factory, interner);
    
    parse_ctx.parse_ast(source_text)
}

/// Produce bytecode from AST
pub fn compile_ast(interner: StringInterner, ast: Vec<StmtMeta>) -> Result<CompiledProgram, Vec<CompileError>> {
    let compiler = Compiler::new(interner);
    compiler.compile_program(ast.iter())
}