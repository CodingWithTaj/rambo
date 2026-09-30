//! Rambo: a small, fast scripting language.
//!
//! Pipeline:
//!
//! ```text
//! source → lexer → tokens → parser → AST → compiler → bytecode → VM
//! ```

pub mod ast;
pub mod bytecode;
pub mod compiler;
pub mod lexer;
pub mod natives;
pub mod parser;
pub mod value;
pub mod vm;

use std::fmt::Write;
use std::rc::Rc;


pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// An error from any stage, with enough position information to point at the source.
#[derive(Debug, Clone)]
pub struct RamboError {
    pub kind: &'static str,
    pub msg: String,
    pub line: u32,
    pub col: u32,
    /// Call stack for runtime errors: (function, line, column), innermost first.
    pub trace: Vec<(String, u32, u32)>,
}

impl std::fmt::Display for RamboError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} (line {}, column {}): {}", self.kind, self.line, self.col, self.msg)
    }
}

impl RamboError {
    /// A friendly, multi-line rendering with the offending line and a caret:
    ///
    /// ```text
    /// runtime error: can't add number and nil
    ///  --> line 3, column 22
    ///   |
    /// 3 |     return total(xs) + nil;
    ///   |                      ^
    ///   = at average (line 3)
    ///     at <script> (line 7)
    /// ```
    pub fn render(&self, src: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "{}: {}", self.kind, self.msg);
        let Some(text) = src.lines().nth(self.line.saturating_sub(1) as usize) else {
            let _ = write!(out, " --> line {}", self.line);
            return out;
        };
        let gutter = self.line.to_string().len();
        let pad = " ".repeat(gutter);
        let _ = writeln!(out, "{} --> line {}, column {}", pad, self.line, self.col);
        let _ = writeln!(out, "{} |", pad);
        let _ = writeln!(out, "{} | {}", self.line, text);
        // keep tabs as tabs so the caret lines up however the terminal renders them
        let lead: String = text.chars().take(self.col.saturating_sub(1) as usize)
            .map(|c| if c == '\t' { '\t' } else { ' ' }).collect();
        let _ = write!(out, "{} | {}^", pad, lead);
        let mut first = true;
        for (name, line, _) in &self.trace {
            let _ = write!(out, "\n{} {} at {} (line {})", pad, if first { "=" } else { " " }, name, line);
            first = false;
        }
        out
    }
}

/// Compile source against an existing set of globals.
pub fn compile_with(src: &str, globals: &mut compiler::Globals, heap: &mut value::Heap, prelude: bool) -> Result<Rc<bytecode::Proto>, RamboError> {
    let err = |kind, msg, line, col| RamboError { kind, msg, line, col, trace: vec![] };
    let toks = lexer::lex(src).map_err(|e| err("syntax error", e.msg, e.line, e.col))?;
    let ast = parser::parse(toks).map_err(|e| err("syntax error", e.msg, e.line, e.col))?;
    compiler::compile(&ast, globals, heap, prelude).map_err(|e| err("compile error", e.msg, e.line, e.col))
}

pub struct RunResult {
    pub output: String,
    pub error: Option<RamboError>,
    pub steps: u64,
}

#[doc(hidden)]
pub fn runtime_error_pub(e: vm::RuntimeError) -> RamboError { runtime_error(e) }

fn runtime_error(e: vm::RuntimeError) -> RamboError {
    let (line, col) = e.trace.first().map(|t| (t.1, t.2)).unwrap_or((0, 0));
    RamboError { kind: "runtime error", msg: e.msg, line, col, trace: e.trace }
}

/// Compile and run a whole program, capturing everything it prints.
pub fn run_source(src: &str, step_limit: Option<u64>) -> RunResult {
    let mut machine = vm::Vm::new();
    machine.step_limit = step_limit;
    if let Err(e) = machine.load(src) {
        return RunResult { output: String::new(), error: Some(e), steps: 0 };
    }
    let error = machine.run(u64::MAX).err().map(runtime_error);
    RunResult { output: std::mem::take(&mut machine.output), error, steps: machine.steps }
}

/// Run a program to completion with a VM the caller owns (command line, REPL).
pub fn run_in(machine: &mut vm::Vm, src: &str) -> Result<(), RamboError> {
    machine.load(src)?;
    machine.run(u64::MAX).map(|_| ()).map_err(runtime_error)
}

pub fn disassemble_source(src: &str) -> Result<String, RamboError> {
    // compile against a real VM so built-in globals show their names
    let mut machine = vm::Vm::new();
    let proto = compile_with(src, &mut machine.globals, &mut machine.heap, false)?;
    Ok(bytecode::disassemble(&proto, &machine.globals.names, &machine.heap))
}

// ------------------------------------------------------------------------
// WebAssembly interface.
//
// No wasm-bindgen: plain exported functions and linear memory, so the
// boundary between JavaScript and Rust is visible rather than generated.
//
//   1. JS calls `alloc(n)`, copies UTF-8 source into that buffer.
//   2. JS calls an entry point (`run`, `disassemble`, `dbg_start`, ...).
//   3. Rust writes a JSON result; JS reads it via `result_ptr`/`result_len`.
//
// The module imports one function, `env.now()`, for clock().
// ------------------------------------------------------------------------
#[cfg(target_arch = "wasm32")]
mod wasm;

/// JSON string escaping (shared by the wasm interface and tests).
pub fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => { let _ = write!(out, "\\u{:04x}", c as u32); }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub mod debug;

