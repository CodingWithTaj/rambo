//! The instruction set, the compiled-function format, and a disassembler.
//!
//! Stack slots: when a function is called, its arguments and local variables
//! live in a window of the VM's value stack starting at the frame's `base`.
//! Slot 0 holds the function being called, parameters follow, then locals
//! in declaration order.

use crate::value::{fmt_num, Heap, Value};
use std::fmt::Write;

/// One VM instruction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Const(u16),
    Nil,
    True,
    False,
    Pop,
    /// Duplicate the top two stack values (used by `list[i] += x`)
    Dup2,
    GetLocal(u16),
    SetLocal(u16),
    GetUpvalue(u16),
    SetUpvalue(u16),
    /// Move the local on top of the stack into its upvalue, then pop it
    CloseUpvalue,
    GetGlobal(u16),
    SetGlobal(u16),
    DefineGlobal(u16),
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Le, Gt, Ge,
    Neg, Not,
    Jump(u32),
    /// Fused compare-and-branch "superinstructions": pop two values and jump
    /// if the comparison is false. The compiler emits these in place of a
    /// comparison followed by JumpIfFalse, which is the shape of almost
    /// every loop condition.
    JumpIfNotLt(u32),
    JumpIfNotLe(u32),
    JumpIfNotGt(u32),
    JumpIfNotGe(u32),
    /// Pops the condition
    JumpIfFalse(u32),
    /// Leaves the value on the stack (used by && and ||)
    JumpIfFalseKeep(u32),
    JumpIfTrueKeep(u32),
    Call(u8),
    Return,
    Closure(u16),
    MakeList(u16),
    MakeMap(u16),
    /// Concatenate N values into a string (string interpolation)
    Concat(u16),
    Index,
    SetIndex,
    /// Turn the value to loop over into something indexable (map → its keys)
    IterPrep,
    Len,
}

#[derive(Default)]
pub struct Chunk {
    pub code: Vec<Op>,
    /// (line, column) for each instruction, for error messages and the debugger
    pub spans: Vec<(u32, u32)>,
    pub consts: Vec<Value>,
    pub protos: Vec<std::rc::Rc<Proto>>,
}

/// Where a named local variable lives, and during which instructions it exists.
#[derive(Clone, Debug)]
pub struct LocalInfo {
    pub name: String,
    pub slot: u16,
    pub start: usize,
    pub end: usize,
}

/// A compiled function.
pub struct Proto {
    pub name: String,
    pub arity: usize,
    pub chunk: Chunk,
    /// For each upvalue: (captures a local of the enclosing function?, index)
    pub upvalues: Vec<(bool, u16)>,
    pub upvalue_names: Vec<String>,
    pub locals: Vec<LocalInfo>,
    /// true for code from the built-in prelude rather than the user's program
    pub is_prelude: bool,
}

impl Proto {
    pub fn span_at(&self, ip: usize) -> (u32, u32) {
        self.chunk.spans.get(ip).copied().unwrap_or((0, 0))
    }
    /// Named locals that exist at instruction `ip`.
    pub fn locals_at(&self, ip: usize) -> impl Iterator<Item = &LocalInfo> {
        self.locals.iter().filter(move |l| l.start <= ip && ip < l.end && !l.name.starts_with('@'))
    }
}

// ------------------------------------------------------------ disassembly

pub fn disassemble(proto: &Proto, globals: &[std::rc::Rc<str>], heap: &Heap) -> String {
    let mut out = String::new();
    dis_proto(proto, globals, heap, &mut out);
    out
}

fn dis_proto(p: &Proto, globals: &[std::rc::Rc<str>], heap: &Heap, out: &mut String) {
    let _ = writeln!(out, "== {} (params: {}, upvalues: {}) ==", p.name, p.arity, p.upvalues.len());
    let mut last_line = 0;
    for (i, op) in p.chunk.code.iter().enumerate() {
        let (line, _) = p.chunk.spans[i];
        let line_col = if line == last_line { "   |".to_string() } else { format!("{:4}", line) };
        last_line = line;
        let (name, args, note) = describe_op(p, i, op, globals, heap);
        let _ = write!(out, "{:04} {}  {:<16}{:<8}", i, line_col, name, args);
        if !note.is_empty() { let _ = write!(out, "; {}", note); }
        out.push('\n');
    }
    for child in &p.chunk.protos {
        out.push('\n');
        dis_proto(child, globals, heap, out);
    }
}

pub fn op_text(p: &Proto, ip: usize, globals: &[std::rc::Rc<str>], heap: &Heap) -> String {
    match p.chunk.code.get(ip) {
        Some(op) => {
            let (name, args, note) = describe_op(p, ip, op, globals, heap);
            let mut s = format!("{} {}", name, args);
            if !note.is_empty() { let _ = write!(s, "  ; {}", note); }
            s.trim_end().to_string()
        }
        None => String::new(),
    }
}

fn const_text(v: &Value, heap: &Heap) -> String {
    match v {
        Value::Num(n) => fmt_num(*n),
        other => heap.repr(other),
    }
}

fn describe_op(p: &Proto, ip: usize, op: &Op, globals: &[std::rc::Rc<str>], heap: &Heap) -> (&'static str, String, String) {
    let e = String::new;
    let local = |s: &u16| {
        p.locals.iter()
            .find(|l| l.slot == *s && l.start <= ip + 1 && ip <= l.end)
            .map(|l| l.name.clone())
            .unwrap_or_default()
    };
    let upval = |i: &u16| p.upvalue_names.get(*i as usize).cloned().unwrap_or_default();
    let global = |i: &u16| globals.get(*i as usize).map(|s| s.to_string()).unwrap_or_default();
    match op {
        Op::Const(i) => ("CONST", i.to_string(), p.chunk.consts.get(*i as usize).map(|c| const_text(c, heap)).unwrap_or_default()),
        Op::Nil => ("NIL", e(), e()),
        Op::True => ("TRUE", e(), e()),
        Op::False => ("FALSE", e(), e()),
        Op::Pop => ("POP", e(), e()),
        Op::Dup2 => ("DUP2", e(), e()),
        Op::GetLocal(s) => ("GET_LOCAL", s.to_string(), local(s)),
        Op::SetLocal(s) => ("SET_LOCAL", s.to_string(), local(s)),
        Op::GetUpvalue(i) => ("GET_UPVALUE", i.to_string(), upval(i)),
        Op::SetUpvalue(i) => ("SET_UPVALUE", i.to_string(), upval(i)),
        Op::CloseUpvalue => ("CLOSE_UPVALUE", e(), e()),
        Op::GetGlobal(i) => ("GET_GLOBAL", i.to_string(), global(i)),
        Op::SetGlobal(i) => ("SET_GLOBAL", i.to_string(), global(i)),
        Op::DefineGlobal(i) => ("DEFINE_GLOBAL", i.to_string(), global(i)),
        Op::Add => ("ADD", e(), e()),
        Op::Sub => ("SUB", e(), e()),
        Op::Mul => ("MUL", e(), e()),
        Op::Div => ("DIV", e(), e()),
        Op::Mod => ("MOD", e(), e()),
        Op::Eq => ("EQ", e(), e()),
        Op::Ne => ("NE", e(), e()),
        Op::Lt => ("LT", e(), e()),
        Op::Le => ("LE", e(), e()),
        Op::Gt => ("GT", e(), e()),
        Op::Ge => ("GE", e(), e()),
        Op::Neg => ("NEG", e(), e()),
        Op::Not => ("NOT", e(), e()),
        Op::Jump(t) => ("JUMP", format!("{:04}", t), e()),
        Op::JumpIfFalse(t) => ("JUMP_IF_FALSE", format!("{:04}", t), e()),
        Op::JumpIfNotLt(t) => ("JUMP_IF_NOT_LT", format!("{:04}", t), "fused LT + JUMP_IF_FALSE".into()),
        Op::JumpIfNotLe(t) => ("JUMP_IF_NOT_LE", format!("{:04}", t), "fused LE + JUMP_IF_FALSE".into()),
        Op::JumpIfNotGt(t) => ("JUMP_IF_NOT_GT", format!("{:04}", t), "fused GT + JUMP_IF_FALSE".into()),
        Op::JumpIfNotGe(t) => ("JUMP_IF_NOT_GE", format!("{:04}", t), "fused GE + JUMP_IF_FALSE".into()),
        Op::JumpIfFalseKeep(t) => ("JUMP_FALSE_KEEP", format!("{:04}", t), e()),
        Op::JumpIfTrueKeep(t) => ("JUMP_TRUE_KEEP", format!("{:04}", t), e()),
        Op::Call(n) => ("CALL", n.to_string(), format!("{} arg{}", n, if *n == 1 { "" } else { "s" })),
        Op::Return => ("RETURN", e(), e()),
        Op::Closure(i) => {
            let child = p.chunk.protos.get(*i as usize);
            let note = child.map(|c| {
                if c.upvalues.is_empty() { c.name.clone() } else { format!("{}, captures {}", c.name, c.upvalue_names.join(", ")) }
            }).unwrap_or_default();
            ("CLOSURE", i.to_string(), note)
        }
        Op::MakeList(n) => ("MAKE_LIST", n.to_string(), e()),
        Op::MakeMap(n) => ("MAKE_MAP", n.to_string(), format!("{} entr{}", n, if *n == 1 { "y" } else { "ies" })),
        Op::Concat(n) => ("CONCAT", n.to_string(), e()),
        Op::Index => ("INDEX", e(), e()),
        Op::SetIndex => ("SET_INDEX", e(), e()),
        Op::IterPrep => ("ITER_PREP", e(), e()),
        Op::Len => ("LEN", e(), e()),
    }
}
