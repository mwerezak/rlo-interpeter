use std::io::{self, Write};
use std::path::PathBuf;
use clap::{Command, Arg, ArgMatches};

use sphinx_lang;
use sphinx_lang::frontend;
use sphinx_lang::BuildErrors;
use sphinx_lang::source::{ModuleSource, SourceType, SourceText};
use sphinx_lang::parser::stmt::{Stmt, StmtMeta};
use sphinx_lang::codegen::Chunk;
use sphinx_lang::runtime::VirtualMachine;
use sphinx_lang::runtime::strings::StringInterner;
use sphinx_lang::debug::symbol::BufferedResolver;

fn main() {
    env_logger::init();
    
    let app = Command::new("sphinx")
        .version("0.0")
        .author("M. Werezak <mwerezak@gmail.com>")
        .about("An interpreter for the Sphinx programming language")
        .arg(
            Arg::new("file")
            .index(1)
            .help("Path to input script file")
            .value_name("FILE")
        )
        .arg(
            Arg::new("cmd")
            .short('c')
            .help("Execute a snippet then exit")
            .value_name("CMD")
        )
        .arg(
            Arg::new("interactive")
            .short('i')
            .help("Drop into an interactive REPL after executing")
        )
        .arg(
            Arg::new("parse_only")
            .short('P')
            .help("Parse and print AST instead of executing")
        )
        .arg(
            Arg::new("compile_only")
            .short('d')
            .help("Produce compiled bytecode instead of executing (not implemented)")
        );
    
    let version = app.get_version().unwrap();
    let args = app.get_matches();
    
    let mut module = None;
    if let Some(s) = args.value_of("cmd") {
        let source = SourceType::String(s.to_string());
        module = Some(ModuleSource::new("<cmd>", source));
    } else if let Some(s) = args.value_of("file") {
        let source = SourceType::File(PathBuf::from(s));
        module = Some(ModuleSource::new(s, source));
    }
    
    if module.is_none() {
        start_repl(&args, version, None);
        return;
    }
    
    let module = module.unwrap();
    
    if args.is_present("parse_only") {
        parse_and_print_ast(&args, module);
    }
    else if args.is_present("compile_only") {
        unimplemented!()
    }
    else {
        let exec_result = build_and_execute(&args, module);
        
        if let Ok(vm) = exec_result {
            if args.is_present("interactive") {
                start_repl(&args, version, Some(vm));
            }
        }
    }
}

fn start_repl(_args: &ArgMatches, version: &str, vm: Option<VirtualMachine>) {
    println!("\nSphinx Version {}\n", version);
    
    let mut repl;
    if let Some(vm) = vm {
        repl = Repl::with_vm(vm);
    } else {
        repl = Repl::new();
    }
    
    repl.run();
}

fn build_and_execute(_args: &ArgMatches, module: ModuleSource) -> Result<VirtualMachine, ()> {
    // build module
    let build_result = sphinx_lang::build_module(&module);
    if build_result.is_err() {
        match build_result.unwrap_err() {
            BuildErrors::Source(error) => {
                println!("Error reading source: {}.", error);
            }
            
            BuildErrors::Syntax(errors) => {
                println!("Errors in file \"{}\":\n", module.name());
                frontend::print_source_errors(&module, &errors);
            }
            
            BuildErrors::Compile(errors) => {
                println!("Errors in file \"{}\":\n", module.name());
                frontend::print_source_errors(&module, &errors);
            }
        }
        return Err(());
    }
    
    let program = build_result.unwrap();
    let chunk = Chunk::load(program.bytecode);
    let mut vm = VirtualMachine::new(chunk);
    
    vm.run().expect("runtime error");
    
    Ok(vm)
}


fn parse_and_print_ast(_args: &ArgMatches, module: ModuleSource) {
    let source_text = match module.source_text() {
        Ok(source_text) => source_text,
        
        Err(error) => {
            println!("Error reading source: {}.", error);
            return;
        },
    };
    
    let mut interner = StringInterner::new();
    let parse_result = sphinx_lang::parse_source(&mut interner, source_text);
    
    match parse_result {
        Err(errors) => {
            println!("Errors in file \"{}\":\n", module.name());
            frontend::print_source_errors(&module, &errors);
        },
        Ok(ast) => println!("{:#?}", ast),
    }
}


//////// REPL ////////


const PROMT_START: &str = ">>> ";
const PROMT_CONTINUE: &str = "... ";

struct Repl {
    vm: Option<VirtualMachine>,
}

enum ReadLine {
    Ok(String),
    Empty,
    Restart,
    Quit,
}

impl Repl {
    pub fn new() -> Self {
        Self { vm: None }
    }
    
    pub fn with_vm(vm: VirtualMachine) -> Self {
        Self { vm: Some(vm) }
    }
    
    fn read_line(&self, prompt: &'static str) -> ReadLine {
        io::stdout().write(prompt.as_bytes()).unwrap();
        io::stdout().flush().unwrap();
        
        let mut input = String::new();
        let result = io::stdin().read_line(&mut input);
        if result.is_err() {
            println!("Could not read input: {}", result.unwrap_err());
            return ReadLine::Restart;
        }
        
        input = input.trim_end().to_string();
        
        if input.is_empty() {
            return ReadLine::Empty;
        }
        
        if input == "quit" || input.chars().find(|c| *c == '\x04').is_some() {
            return ReadLine::Quit;
        }
        
        ReadLine::Ok(input)
    }
    
    pub fn run(&mut self) {
        
        loop {
            let mut interner;
            let mut input = String::new();
            let mut parse_result = None;
            
            loop {
                let prompt =
                    if input.is_empty() { PROMT_START }
                    else { PROMT_CONTINUE };
                
                interner = StringInterner::new();
                
                match self.read_line(prompt) {
                    ReadLine::Quit => return,
                    ReadLine::Restart => continue,
                    ReadLine::Empty => {
                        if input.is_empty() { continue }
                        else { break }
                    },
                    ReadLine::Ok(line) => {
                        input.push_str(&line);
                        
                        if line.trim_end().ends_with(';') {
                            break
                        }
                        
                        // If we can't parse the input without errors, then we assume we need to continue
                        let source_text = SourceText::from(input.clone());
                        if let Ok(ast) = sphinx_lang::parse_source(&mut interner, source_text) {
                            parse_result.replace(ast);
                            break
                        }
                        
                        input.push('\n')
                    }
                }
            }
            
            let parse_result =
                if let Some(ast) = parse_result { Ok(ast) }
                else { 
                    let source_text = SourceText::from(input.clone());
                    sphinx_lang::parse_source(&mut interner, source_text) 
                };
            
            let mut ast = match parse_result {
                Ok(ast) => ast,
                
                Err(errors) => {
                    let resolver = BufferedResolver::new(input);
                    frontend::print_source_errors(&resolver, &errors);
                    continue;
                },
            };
            
            // if the last stmt is an expression statement, convert it into an inspect
            if let Some(stmt) = ast.pop() {
                let (mut stmt, symbol) = stmt.take();
                if let Stmt::Expression(expr) = stmt {
                    stmt = Stmt::Echo(expr);
                }
                ast.push(StmtMeta::new(stmt, symbol))
            }
            
            let program = match sphinx_lang::compile_ast(interner, ast) {
                Ok(program) => program,
                
                Err(errors) => {
                    let resolver = BufferedResolver::new(input);
                    frontend::print_source_errors(&resolver, &errors);
                    continue;
                }
            };
            
            let chunk = Chunk::load(program.bytecode);
            match self.vm {
                Some(ref mut vm) => vm.reload_program(chunk),
                None => { self.vm.replace(VirtualMachine::new(chunk)); },
            }
            
            if let Err(error) = self.vm.as_mut().unwrap().run() {
                println!("Runtime error: {:?}", error);
            }
            
            // for stmt in stmts.iter() {
            //     match stmt.variant() {
            //         Stmt::Expression(expr) => {
            //             let eval_ctx = EvalContext::new(&self.root_env);
            //             let eval_result = eval_ctx.eval_expr(&expr);
            //             log::debug!("{:?}", eval_result);
            //             match eval_result {
            //                 Ok(value) => {
            //                     println!("{}", value.unwrap_value());
            //                 },
            //                 Err(error) => {
            //                     println!("{:?}", error)
            //                 },
            //             }
            //         },
            //         _ => {
            //             let exec_ctx = ExecContext::new(&self.root_env);
            //             let exec_result = exec_ctx.exec(&stmt);
            //             log::debug!("{:?}", exec_result);
            //         },
            //     }
            // }
            
        }
        
    }
}