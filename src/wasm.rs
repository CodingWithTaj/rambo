//! WebAssembly exports. See the comment in lib.rs for the calling convention.

use crate::debug::{Session, StepMode};
use crate::json_str;
use std::cell::RefCell;

thread_local! {
    static RESULT: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    static SESSION: RefCell<Option<(Session, String)>> = RefCell::new(None);
}

fn set_result(s: String) {
    RESULT.with(|r| *r.borrow_mut() = s.into_bytes());
}

unsafe fn take_input(ptr: *mut u8, len: usize) -> String {
    let bytes = Vec::from_raw_parts(ptr, len, len.max(1)); // owned now; freed on drop
    String::from_utf8_lossy(&bytes).into_owned()
}

fn limit(n: f64) -> Option<u64> { if n > 0.0 { Some(n as u64) } else { None } }

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub extern "C" fn result_ptr() -> *const u8 { RESULT.with(|r| r.borrow().as_ptr()) }

#[no_mangle]
pub extern "C" fn result_len() -> usize { RESULT.with(|r| r.borrow().len()) }

/// Run a whole program. `step_limit` 0 means unlimited.
#[no_mangle]
pub unsafe extern "C" fn run(ptr: *mut u8, len: usize, step_limit: f64) {
    let src = take_input(ptr, len);
    let r = crate::run_source(&src, limit(step_limit));
    let (err, line) = match &r.error {
        Some(e) => (e.render(&src), e.line),
        None => (String::new(), 0),
    };
    set_result(format!(
        "{{\"ok\":{},\"output\":{},\"error\":{},\"line\":{},\"steps\":{}}}",
        r.error.is_none(), json_str(&r.output), json_str(&err), line, r.steps
    ));
}

#[no_mangle]
pub unsafe extern "C" fn disassemble(ptr: *mut u8, len: usize) {
    let src = take_input(ptr, len);
    set_result(match crate::disassemble_source(&src) {
        Ok(text) => format!("{{\"ok\":true,\"output\":{}}}", json_str(&text)),
        Err(e) => format!("{{\"ok\":false,\"output\":{},\"line\":{}}}", json_str(&e.render(&src)), e.line),
    });
}

/// Start a debugging session, paused before the first instruction.
#[no_mangle]
pub unsafe extern "C" fn dbg_start(ptr: *mut u8, len: usize, step_limit: f64) {
    let src = take_input(ptr, len);
    match Session::start(&src, limit(step_limit)) {
        Ok(s) => {
            let json = s.state_json(&src);
            SESSION.with(|cell| *cell.borrow_mut() = Some((s, src)));
            set_result(json);
        }
        Err(e) => {
            SESSION.with(|cell| *cell.borrow_mut() = None);
            set_result(format!("{{\"status\":\"error\",\"error\":{},\"errorLine\":{}}}", json_str(&e.render(&src)), e.line));
        }
    }
}

/// Breakpoints as a comma-separated list of line numbers.
#[no_mangle]
pub unsafe extern "C" fn dbg_breakpoints(ptr: *mut u8, len: usize) {
    let text = take_input(ptr, len);
    let lines: Vec<u32> = text.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    SESSION.with(|cell| {
        if let Some((s, _)) = cell.borrow_mut().as_mut() { s.breakpoints = lines; }
    });
    set_result("{}".into());
}

/// mode: 0 instruction, 1 step into, 2 step over, 3 step out, 4 continue
#[no_mangle]
pub extern "C" fn dbg_step(mode: u32) {
    let json = SESSION.with(|cell| match cell.borrow_mut().as_mut() {
        Some((s, src)) => { s.step(StepMode::from_code(mode)); s.state_json(src) }
        None => "{\"status\":\"none\"}".into(),
    });
    set_result(json);
}

#[no_mangle]
pub extern "C" fn dbg_stop() {
    SESSION.with(|cell| *cell.borrow_mut() = None);
    set_result("{}".into());
}
