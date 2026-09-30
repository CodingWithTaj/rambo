//! The `rambo` command.

use std::io::{self, BufRead, Write};
use std::process::ExitCode;

const HELP: &str = "\
Rambo — a small, fast scripting language

USAGE:
    rambo                     start the interactive REPL
    rambo <file.rmb>          run a program
    rambo dis <file.rmb>      show the bytecode a program compiles to
    rambo builtins            list the built-in functions
    rambo --version           print the version
    rambo --help              print this help
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => { repl(); ExitCode::SUCCESS }
        ["--help" | "-h" | "help"] => { print!("{}", HELP); ExitCode::SUCCESS }
        ["--version" | "-V" | "version"] => { println!("rambo {}", rambo::VERSION); ExitCode::SUCCESS }
        ["builtins"] => { builtins(); ExitCode::SUCCESS }
        ["dis" | "--dis", path] => match read(path) {
            Some(src) => match rambo::disassemble_source(&src) {
                Ok(text) => { print!("{}", text); ExitCode::SUCCESS }
                Err(e) => { eprintln!("{}", e.render(&src)); ExitCode::from(65) }
            },
            None => ExitCode::from(66),
        },
        [path] if !path.starts_with('-') => match read(path) {
            Some(src) => run_file(&src),
            None => ExitCode::from(66),
        },
        _ => {
            eprint!("{}", HELP);
            ExitCode::from(64)
        }
    }
}

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path).map_err(|e| eprintln!("error: can't read {}: {}", path, e)).ok()
}

fn run_file(src: &str) -> ExitCode {
    let mut vm = rambo::vm::Vm::new();
    vm.echo = true;
    match rambo::run_in(&mut vm, src) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            io::stdout().flush().ok();
            eprintln!("{}", e.render(src));
            ExitCode::from(if e.kind == "runtime error" { 70 } else { 65 })
        }
    }
}

fn builtins() {
    println!("Written in Rust:");
    for n in rambo::natives::NATIVES {
        println!("  {:<40} {}", n.sig, n.doc);
    }
    println!("\nWritten in Rambo (the prelude):");
    println!("  {}", rambo::natives::PRELUDE_NAMES.join(", "));
}

fn repl() {
    println!("Rambo {} REPL. Top-level variables persist between entries.", rambo::VERSION);
    println!("Type an expression to see its value. Ctrl-D (or Ctrl-Z then Enter on Windows) to exit.");
    let mut vm = rambo::vm::Vm::new();
    vm.echo = true;
    let stdin = io::stdin();
    let mut buffer = String::new();
    loop {
        print!("{}", if buffer.is_empty() { "rambo> " } else { "  ...> " });
        io::stdout().flush().ok();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 { println!(); break; }
        buffer.push_str(&line);

        // keep reading while brackets are unbalanced, so multi-line functions work
        let depth: i32 = buffer.chars().map(|c| match c { '{' | '(' | '[' => 1, '}' | ')' | ']' => -1, _ => 0 }).sum();
        if depth > 0 { continue; }

        let src = buffer.trim().to_string();
        buffer.clear();
        if src.is_empty() { continue; }

        // a bare expression like `1 + 2` prints its value
        let is_expression = !src.ends_with(';') && !src.ends_with('}');
        let program = if is_expression { format!("print({});", src) } else { src };
        if let Err(e) = rambo::run_in(&mut vm, &program) {
            io::stdout().flush().ok();
            eprintln!("{}", e.render(&program));
        }
    }
}
