//! The step-through debugger behind the playground.
//!
//! It lives here rather than in the WebAssembly glue so it can be tested
//! natively. A `Session` owns a VM that's paused between instructions;
//! each step command runs single instructions until the stopping rule for
//! that command is met.

use crate::vm::{Status, Vm};
use crate::{json_str, RamboError};
use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StepMode {
    /// One bytecode instruction.
    Instruction,
    /// Until the source line changes, entering function calls.
    Into,
    /// Until the next line in this function (or a return to the caller).
    Over,
    /// Until the current function returns.
    Out,
    /// Until a breakpoint line is reached, or the program ends.
    Continue,
}

impl StepMode {
    pub fn from_code(n: u32) -> StepMode {
        match n {
            0 => StepMode::Instruction,
            1 => StepMode::Into,
            2 => StepMode::Over,
            3 => StepMode::Out,
            _ => StepMode::Continue,
        }
    }
}

/// Upper bound on instructions for a single step command, so a stray
/// infinite loop can't freeze the page.
const MAX_PER_COMMAND: u64 = 20_000_000;

pub struct Session {
    pub vm: Vm,
    pub breakpoints: Vec<u32>,
    pub error: Option<RamboError>,
    pub finished: bool,
    pub note: String,
}

impl Session {
    pub fn start(src: &str, step_limit: Option<u64>) -> Result<Session, RamboError> {
        let mut vm = Vm::new();
        vm.step_limit = step_limit;
        vm.load(src)?;
        Ok(Session { vm, breakpoints: vec![], error: None, finished: false, note: String::new() })
    }

    fn depth(&self) -> usize { self.vm.frames.len() }
    fn line(&self) -> u32 { self.vm.position().map(|p| p.0).unwrap_or(0) }
    fn in_prelude(&self) -> bool { self.vm.frames.last().is_some_and(|f| f.proto.is_prelude) }

    pub fn step(&mut self, mode: StepMode) {
        self.note.clear();
        if self.finished { return; }
        let (start_depth, start_line) = (self.depth(), self.line());
        let (mut prev_depth, mut prev_line) = (start_depth, start_line);
        let mut count = 0u64;
        loop {
            match self.vm.run(1) {
                Err(e) => {
                    self.error = Some(crate::runtime_error_pub(e));
                    self.finished = true;
                    return;
                }
                Ok(Status::Finished) => { self.finished = true; return; }
                Ok(Status::Paused) => {}
            }
            count += 1;
            if count >= MAX_PER_COMMAND {
                self.note = format!("paused after {} instructions without reaching a stopping point", count);
                return;
            }
            if self.vm.frames.is_empty() { self.finished = true; return; }
            if self.in_prelude() { continue; } // library code isn't in the editor, so run through it

            let (d, l) = (self.depth(), self.line());
            let stop = match mode {
                StepMode::Instruction => true,
                StepMode::Into => l != start_line || d != start_depth,
                StepMode::Over => d < start_depth || (d == start_depth && l != start_line),
                StepMode::Out => d < start_depth,
                StepMode::Continue => self.breakpoints.contains(&l) && (l != prev_line || d != prev_depth),
            };
            prev_depth = d;
            prev_line = l;
            if stop { return; }
        }
    }

    fn repr(&self, v: &crate::value::Value) -> String {
        let s = self.vm.heap.repr(v);
        if s.chars().count() > 120 { format!("{}…", s.chars().take(119).collect::<String>()) } else { s }
    }

    /// Everything the debugger panel shows, as JSON.
    pub fn state_json(&self, src: &str) -> String {
        let vm = &self.vm;
        let mut j = String::from("{");
        let status = if self.error.is_some() { "error" } else if self.finished { "finished" } else { "paused" };
        let _ = write!(j, "\"status\":\"{}\",", status);
        let _ = write!(j, "\"output\":{},", json_str(&vm.output));
        let _ = write!(j, "\"steps\":{},", vm.steps);
        let _ = write!(j, "\"note\":{},", json_str(&self.note));
        let _ = write!(j, "\"heap\":{{\"live\":{},\"collections\":{}}},", vm.heap.live(), vm.heap.collections);
        match &self.error {
            Some(e) => { let _ = write!(j, "\"error\":{},\"errorLine\":{},", json_str(&e.render(src)), e.line); }
            None => { j.push_str("\"error\":\"\",\"errorLine\":0,"); }
        }

        // current position
        if let Some(f) = vm.frames.last() {
            let (line, col) = f.proto.span_at(f.ip);
            let op = crate::bytecode::op_text(&f.proto, f.ip, &vm.globals.names, &vm.heap);
            let _ = write!(j, "\"line\":{},\"col\":{},\"ip\":{},\"fn\":{},\"op\":{},", line, col, f.ip, json_str(&f.proto.name), json_str(&op));
        } else {
            j.push_str("\"line\":0,\"col\":0,\"ip\":0,\"fn\":\"\",\"op\":\"\",");
        }

        // call frames, innermost first (library frames hidden)
        j.push_str("\"frames\":[");
        let mut first = true;
        for f in vm.frames.iter().rev().filter(|f| !f.proto.is_prelude) {
            if !first { j.push(','); }
            first = false;
            let (line, _) = f.proto.span_at(f.ip.saturating_sub(if std::ptr::eq(f, vm.frames.last().unwrap()) { 0 } else { 1 }));
            let _ = write!(j, "{{\"name\":{},\"line\":{},\"locals\":[", json_str(&f.proto.name), line);
            let mut firstl = true;
            for l in f.proto.locals_at(f.ip) {
                if l.slot == 0 { continue; }
                let Some(v) = vm.stack.get(f.base + l.slot as usize) else { continue };
                if !firstl { j.push(','); }
                firstl = false;
                let _ = write!(j, "[{},{}]", json_str(&l.name), json_str(&self.repr(v)));
            }
            j.push_str("],\"captured\":[");
            let mut firstc = true;
            for (name, v) in vm.frame_upvalues(f) {
                if !firstc { j.push(','); }
                firstc = false;
                let _ = write!(j, "[{},{}]", json_str(&name), json_str(&self.repr(&v)));
            }
            j.push_str("]}");
        }
        j.push_str("],");

        // the value stack (slot 0 holds the script itself, so skip it)
        j.push_str("\"stack\":[");
        let from = vm.stack.len().saturating_sub(40).max(1);
        for (i, v) in vm.stack.iter().enumerate().skip(from) {
            if i > from { j.push(','); }
            let _ = write!(j, "{}", json_str(&self.repr(v)));
        }
        j.push_str("],");

        j.push_str("\"globals\":[");
        for (i, (name, v)) in vm.user_globals().iter().enumerate() {
            if i > 0 { j.push(','); }
            let _ = write!(j, "[{},{}]", json_str(name), json_str(&self.repr(v)));
        }
        j.push_str("]}");
        j
    }
}
