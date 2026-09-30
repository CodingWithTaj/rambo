//! Compiler: walks the AST once and emits bytecode.
//!
//! How variables are resolved
//! --------------------------
//! * **Locals** live in stack slots. The compiler knows the slot number of
//!   every local, so reading one is a single array access at runtime.
//! * **Upvalues** are locals of an *enclosing* function that a closure uses.
//!   The compiler records, for each closure, which outer variables it needs;
//!   at runtime the VM links them up when the closure is created. This is
//!   the same design Lua uses.
//! * **Globals** are `let` and `fn` declarations at the top level of a file.
//!   Each global name gets a numbered slot at compile time, so lookups are
//!   array accesses too, while functions can still call each other
//!   regardless of the order they're declared in.

use crate::ast::*;
use crate::bytecode::*;
use crate::value::{Heap, Value};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct CompileError {
    pub msg: String,
    pub line: u32,
    pub col: u32,
}

type CResult<T = ()> = Result<T, CompileError>;

/// Global variable names, shared between the compiler and the VM so that
/// separately compiled code (the prelude, REPL lines) sees the same globals.
#[derive(Default)]
pub struct Globals {
    pub names: Vec<Rc<str>>,
    map: HashMap<Rc<str>, u16>,
}

impl Globals {
    pub fn slot(&mut self, name: &str) -> Option<u16> {
        if let Some(&i) = self.map.get(name) { return Some(i); }
        if self.names.len() >= u16::MAX as usize { return None; }
        let rc: Rc<str> = Rc::from(name);
        let i = self.names.len() as u16;
        self.names.push(rc.clone());
        self.map.insert(rc, i);
        Some(i)
    }
    pub fn lookup(&self, name: &str) -> Option<u16> {
        self.map.get(name).copied()
    }
}

struct Local {
    name: String,
    depth: i32,
    captured: bool,
    info: usize,
}

struct LoopCtx {
    /// locals deeper than this belong to the loop body and are popped on break/continue
    keep_depth: i32,
    breaks: Vec<usize>,
    continues: Vec<usize>,
}

struct FnState {
    name: String,
    arity: usize,
    chunk: Chunk,
    locals: Vec<Local>,
    infos: Vec<LocalInfo>,
    scope_depth: i32,
    upvalues: Vec<(bool, u16)>,
    upvalue_names: Vec<String>,
    loops: Vec<LoopCtx>,
    is_script: bool,
    /// Instruction indexes that some jump lands on (fusion must not cross these).
    jump_targets: Vec<usize>,
}

impl FnState {
    fn new(name: String, arity: usize, is_script: bool) -> Self {
        let mut f = FnState {
            name, arity,
            chunk: Chunk::default(),
            locals: Vec::new(),
            infos: Vec::new(),
            scope_depth: if is_script { 0 } else { 1 },
            upvalues: Vec::new(),
            upvalue_names: Vec::new(),
            loops: Vec::new(),
            is_script,
            jump_targets: Vec::new(),
        };
        // slot 0 holds the function being called
        f.infos.push(LocalInfo { name: "@fn".into(), slot: 0, start: 0, end: usize::MAX });
        f.locals.push(Local { name: String::new(), depth: 0, captured: false, info: 0 });
        f
    }
}

pub struct Compiler<'g> {
    fns: Vec<FnState>,
    span: (u32, u32),
    globals: &'g mut Globals,
    /// String constants are interned into the VM's heap as they're compiled.
    heap: &'g mut Heap,
    prelude: bool,
}

pub fn compile(program: &[Stmt], globals: &mut Globals, heap: &mut Heap, prelude: bool) -> CResult<Rc<Proto>> {
    let mut c = Compiler {
        fns: vec![FnState::new("<script>".into(), 0, true)],
        span: (1, 1),
        globals,
        heap,
        prelude,
    };
    for s in program {
        c.stmt(s)?;
    }
    c.emit(Op::Nil);
    c.emit(Op::Return);
    Ok(Rc::new(c.finish_fn()))
}

impl<'g> Compiler<'g> {
    // ---------- helpers ----------

    fn cur(&mut self) -> &mut FnState { self.fns.last_mut().unwrap() }
    fn cur_ref(&self) -> &FnState { self.fns.last().unwrap() }

    fn err<T>(&self, msg: impl Into<String>) -> CResult<T> {
        Err(CompileError { msg: msg.into(), line: self.span.0, col: self.span.1 })
    }

    fn at(&mut self, span: Span) { self.span = (span.line, span.col); }

    fn emit(&mut self, op: Op) -> usize {
        let span = self.span;
        let chunk = &mut self.cur().chunk;
        chunk.code.push(op);
        chunk.spans.push(span);
        chunk.code.len() - 1
    }

    fn here(&self) -> usize { self.cur_ref().chunk.code.len() }

    /// Emit a conditional jump, fusing it with a comparison just before it
    /// when that's safe (no other jump lands between the two).
    fn emit_jump_if_false(&mut self) -> usize {
        let here = self.here();
        let f = self.cur_ref();
        let fusable = here > 0 && !f.jump_targets.contains(&here);
        if fusable {
            let fused = match f.chunk.code[here - 1] {
                Op::Lt => Some(Op::JumpIfNotLt(0)),
                Op::Le => Some(Op::JumpIfNotLe(0)),
                Op::Gt => Some(Op::JumpIfNotGt(0)),
                Op::Ge => Some(Op::JumpIfNotGe(0)),
                _ => None,
            };
            if let Some(op) = fused {
                // keep the comparison's source position for error messages
                self.cur().chunk.code[here - 1] = op;
                return here - 1;
            }
        }
        self.emit(Op::JumpIfFalse(0))
    }

    fn patch(&mut self, at: usize, target: usize) {
        self.cur().jump_targets.push(target);
        let t = target as u32;
        let op = &mut self.cur().chunk.code[at];
        *op = match *op {
            Op::Jump(_) => Op::Jump(t),
            Op::JumpIfFalse(_) => Op::JumpIfFalse(t),
            Op::JumpIfNotLt(_) => Op::JumpIfNotLt(t),
            Op::JumpIfNotLe(_) => Op::JumpIfNotLe(t),
            Op::JumpIfNotGt(_) => Op::JumpIfNotGt(t),
            Op::JumpIfNotGe(_) => Op::JumpIfNotGe(t),
            Op::JumpIfFalseKeep(_) => Op::JumpIfFalseKeep(t),
            Op::JumpIfTrueKeep(_) => Op::JumpIfTrueKeep(t),
            other => unreachable!("patching non-jump {:?}", other),
        };
    }

    fn constant(&mut self, v: Value) -> CResult<u16> {
        let consts = &mut self.cur().chunk.consts;
        if let Some(i) = consts.iter().position(|c| match (c, &v) {
            (Value::Num(a), Value::Num(b)) => a.to_bits() == b.to_bits(),
            (Value::Obj(a), Value::Obj(b)) => a == b, // interned strings
            _ => false,
        }) {
            return Ok(i as u16);
        }
        if consts.len() >= u16::MAX as usize { return self.err("too many constants in one function"); }
        consts.push(v);
        Ok((consts.len() - 1) as u16)
    }

    fn emit_const(&mut self, v: Value) -> CResult {
        let k = self.constant(v)?;
        self.emit(Op::Const(k));
        Ok(())
    }

    fn global_slot(&mut self, name: &str) -> CResult<u16> {
        match self.globals.slot(name) {
            Some(i) => Ok(i),
            None => self.err("too many global variables"),
        }
    }

    fn is_global_scope(&self) -> bool {
        let f = self.cur_ref();
        f.is_script && f.scope_depth == 0
    }

    // ---------- scopes and locals ----------

    fn add_local(&mut self, name: &str) -> CResult<u16> {
        if self.cur_ref().locals.len() >= u16::MAX as usize {
            return self.err("too many local variables in one function");
        }
        let start = self.here();
        let f = self.cur();
        let slot = f.locals.len() as u16;
        f.infos.push(LocalInfo { name: name.to_string(), slot, start, end: usize::MAX });
        let info = f.infos.len() - 1;
        let depth = f.scope_depth;
        f.locals.push(Local { name: name.to_string(), depth, captured: false, info });
        Ok(slot)
    }

    fn begin_scope(&mut self) { self.cur().scope_depth += 1; }

    fn end_scope(&mut self) {
        self.cur().scope_depth -= 1;
        loop {
            let f = self.cur_ref();
            let Some(last) = f.locals.last() else { break };
            if last.depth <= f.scope_depth { break; }
            let op = if last.captured { Op::CloseUpvalue } else { Op::Pop };
            self.emit(op);
            let end = self.here();
            let f = self.cur();
            let local = f.locals.pop().unwrap();
            f.infos[local.info].end = end;
        }
    }

    /// Before a break/continue jumps out of the loop body, pop its locals.
    fn pop_locals_deeper_than(&mut self, depth: i32) {
        let ops: Vec<Op> = self.cur_ref().locals.iter().rev()
            .take_while(|l| l.depth > depth)
            .map(|l| if l.captured { Op::CloseUpvalue } else { Op::Pop })
            .collect();
        for op in ops { self.emit(op); }
    }

    fn block(&mut self, stmts: &[Stmt]) -> CResult {
        self.begin_scope();
        for s in stmts { self.stmt(s)?; }
        self.end_scope();
        Ok(())
    }

    fn resolve_local(&self, fn_idx: usize, name: &str) -> Option<u16> {
        self.fns[fn_idx].locals.iter().enumerate().rev()
            .find(|(_, l)| l.name == name)
            .map(|(i, _)| i as u16)
    }

    fn resolve_upvalue(&mut self, fn_idx: usize, name: &str) -> CResult<Option<u16>> {
        if fn_idx == 0 { return Ok(None); }
        if let Some(slot) = self.resolve_local(fn_idx - 1, name) {
            self.fns[fn_idx - 1].locals[slot as usize].captured = true;
            return self.add_upvalue(fn_idx, true, slot, name).map(Some);
        }
        if let Some(up) = self.resolve_upvalue(fn_idx - 1, name)? {
            return self.add_upvalue(fn_idx, false, up, name).map(Some);
        }
        Ok(None)
    }

    fn add_upvalue(&mut self, fn_idx: usize, is_local: bool, index: u16, name: &str) -> CResult<u16> {
        let f = &mut self.fns[fn_idx];
        if let Some(i) = f.upvalues.iter().position(|u| *u == (is_local, index)) {
            return Ok(i as u16);
        }
        if f.upvalues.len() >= u16::MAX as usize { return self.err("too many captured variables"); }
        f.upvalues.push((is_local, index));
        f.upvalue_names.push(name.to_string());
        Ok((f.upvalues.len() - 1) as u16)
    }

    fn emit_get(&mut self, name: &str) -> CResult {
        let top = self.fns.len() - 1;
        if let Some(s) = self.resolve_local(top, name) {
            self.emit(Op::GetLocal(s));
        } else if let Some(u) = self.resolve_upvalue(top, name)? {
            self.emit(Op::GetUpvalue(u));
        } else {
            let g = self.global_slot(name)?;
            self.emit(Op::GetGlobal(g));
        }
        Ok(())
    }

    fn emit_set(&mut self, name: &str) -> CResult {
        let top = self.fns.len() - 1;
        if let Some(s) = self.resolve_local(top, name) {
            self.emit(Op::SetLocal(s));
        } else if let Some(u) = self.resolve_upvalue(top, name)? {
            self.emit(Op::SetUpvalue(u));
        } else {
            let g = self.global_slot(name)?;
            self.emit(Op::SetGlobal(g));
        }
        Ok(())
    }

    fn finish_fn(&mut self) -> Proto {
        let mut f = self.fns.pop().unwrap();
        let end = f.chunk.code.len();
        for info in f.infos.iter_mut() {
            if info.end == usize::MAX { info.end = end; }
        }
        Proto {
            name: f.name,
            arity: f.arity,
            chunk: f.chunk,
            upvalues: f.upvalues,
            upvalue_names: f.upvalue_names,
            locals: f.infos,
            is_prelude: self.prelude,
        }
    }

    // ---------- statements ----------

    fn stmt(&mut self, s: &Stmt) -> CResult {
        self.at(s.span);
        match &s.kind {
            StmtKind::Let(name, value) => {
                self.expr(value)?;
                self.at(s.span);
                if self.is_global_scope() {
                    let g = self.global_slot(name)?;
                    self.emit(Op::DefineGlobal(g));
                } else {
                    // the value is already on the stack: that slot *is* the variable
                    self.add_local(name)?;
                }
            }
            StmtKind::Fn(decl) => {
                let name = decl.name.clone().unwrap();
                if self.is_global_scope() {
                    self.function(decl)?;
                    let g = self.global_slot(&name)?;
                    self.emit(Op::DefineGlobal(g));
                } else {
                    // declared before its body so it can call itself
                    self.add_local(&name)?;
                    self.function(decl)?;
                }
            }
            StmtKind::Assign { target, op, value } => self.assign(target, *op, value, s.span)?,
            StmtKind::Expr(e) => {
                self.expr(e)?;
                self.emit(Op::Pop);
            }
            StmtKind::Block(stmts) => self.block(stmts)?,
            StmtKind::If(cond, then, els) => {
                self.expr(cond)?;
                let to_else = self.emit_jump_if_false();
                self.block(then)?;
                if let Some(els) = els {
                    let to_end = self.emit(Op::Jump(0));
                    let here = self.here();
                    self.patch(to_else, here);
                    self.block(els)?;
                    let here = self.here();
                    self.patch(to_end, here);
                } else {
                    let here = self.here();
                    self.patch(to_else, here);
                }
            }
            StmtKind::While(cond, body) => {
                let start = self.here();
                self.cur().jump_targets.push(start);
                self.expr(cond)?;
                let exit = self.emit_jump_if_false();
                let keep = self.cur_ref().scope_depth;
                self.cur().loops.push(LoopCtx { keep_depth: keep, breaks: vec![], continues: vec![] });
                self.block(body)?;
                let ctx = self.cur().loops.pop().unwrap();
                for j in ctx.continues { self.patch(j, start); }
                self.at(s.span);
                self.emit(Op::Jump(start as u32));
                let end = self.here();
                self.patch(exit, end);
                for j in ctx.breaks { self.patch(j, end); }
            }
            StmtKind::ForRange(var, from, to, body) => {
                self.begin_scope();
                self.expr(from)?;
                let i = self.add_local("@i")?;
                self.expr(to)?;
                let end_slot = self.add_local("@end")?;
                self.at(s.span);
                let start = self.here();
                self.cur().jump_targets.push(start);
                self.emit(Op::GetLocal(i));
                self.emit(Op::GetLocal(end_slot));
                self.emit(Op::Lt);
                let exit = self.emit_jump_if_false();
                self.loop_body(var, body, |c| { c.emit(Op::GetLocal(i)); Ok(()) }, |c| {
                    c.emit(Op::GetLocal(i));
                    c.emit_const(Value::Num(1.0))?;
                    c.emit(Op::Add);
                    c.emit(Op::SetLocal(i));
                    Ok(())
                }, start, exit, s.span)?;
                self.at(s.span);
                self.end_scope();
            }
            StmtKind::ForEach(var, iterable, body) => {
                self.begin_scope();
                self.expr(iterable)?;
                self.at(s.span);
                self.emit(Op::IterPrep);
                let seq = self.add_local("@seq")?;
                self.emit_const(Value::Num(0.0))?;
                let idx = self.add_local("@idx")?;
                let start = self.here();
                self.cur().jump_targets.push(start);
                self.emit(Op::GetLocal(idx));
                self.emit(Op::GetLocal(seq));
                self.emit(Op::Len);
                self.emit(Op::Lt);
                let exit = self.emit_jump_if_false();
                self.loop_body(var, body, |c| {
                    c.emit(Op::GetLocal(seq));
                    c.emit(Op::GetLocal(idx));
                    c.emit(Op::Index);
                    Ok(())
                }, |c| {
                    c.emit(Op::GetLocal(idx));
                    c.emit_const(Value::Num(1.0))?;
                    c.emit(Op::Add);
                    c.emit(Op::SetLocal(idx));
                    Ok(())
                }, start, exit, s.span)?;
                self.at(s.span);
                self.end_scope();
            }
            StmtKind::Break => {
                let Some(keep) = self.cur_ref().loops.last().map(|l| l.keep_depth) else {
                    return self.err("'break' can only be used inside a loop");
                };
                self.pop_locals_deeper_than(keep);
                let j = self.emit(Op::Jump(0));
                self.cur().loops.last_mut().unwrap().breaks.push(j);
            }
            StmtKind::Continue => {
                let Some(keep) = self.cur_ref().loops.last().map(|l| l.keep_depth) else {
                    return self.err("'continue' can only be used inside a loop");
                };
                self.pop_locals_deeper_than(keep);
                let j = self.emit(Op::Jump(0));
                self.cur().loops.last_mut().unwrap().continues.push(j);
            }
            StmtKind::Return(value) => {
                if self.cur_ref().is_script { return self.err("'return' can only be used inside a function"); }
                match value {
                    Some(v) => self.expr(v)?,
                    None => { self.emit(Op::Nil); }
                }
                self.at(s.span);
                self.emit(Op::Return);
            }
        }
        Ok(())
    }

    /// Shared shape of both `for` loops. The loop variable is a fresh local in
    /// each iteration, so closures created in the body capture that
    /// iteration's value, not a variable shared by every iteration.
    fn loop_body(
        &mut self,
        var: &str,
        body: &[Stmt],
        load_var: impl FnOnce(&mut Self) -> CResult,
        step: impl FnOnce(&mut Self) -> CResult,
        start: usize,
        exit: usize,
        header: Span,
    ) -> CResult {
        let keep = self.cur_ref().scope_depth;
        self.cur().loops.push(LoopCtx { keep_depth: keep, breaks: vec![], continues: vec![] });
        self.begin_scope();
        load_var(self)?;
        self.add_local(var)?;
        for s in body { self.stmt(s)?; }
        self.end_scope();
        let ctx = self.cur().loops.pop().unwrap();
        self.at(header); // loop bookkeeping belongs to the `for` line, not the body
        let step_at = self.here();
        for j in ctx.continues { self.patch(j, step_at); }
        step(self)?;
        self.emit(Op::Jump(start as u32));
        let end = self.here();
        self.patch(exit, end);
        for j in ctx.breaks { self.patch(j, end); }
        Ok(())
    }

    fn assign(&mut self, target: &Expr, op: Option<BinOp>, value: &Expr, span: Span) -> CResult {
        match &target.kind {
            ExprKind::Var(name) => {
                if let Some(op) = op {
                    self.at(target.span);
                    self.emit_get(name)?;
                    self.expr(value)?;
                    self.at(span);
                    self.emit(binop(op));
                } else {
                    self.expr(value)?;
                }
                self.at(span);
                self.emit_set(name)
            }
            ExprKind::Index(obj, idx) => {
                self.expr(obj)?;
                self.expr(idx)?;
                if let Some(op) = op {
                    self.at(target.span);
                    self.emit(Op::Dup2);
                    self.emit(Op::Index);
                    self.expr(value)?;
                    self.at(span);
                    self.emit(binop(op));
                } else {
                    self.expr(value)?;
                }
                self.at(target.span);
                self.emit(Op::SetIndex);
                Ok(())
            }
            _ => self.err("invalid assignment target"),
        }
    }

    fn function(&mut self, decl: &FnDecl) -> CResult {
        let outer_span = self.span;
        let name = decl.name.clone().unwrap_or_else(|| "<anonymous>".into());
        self.fns.push(FnState::new(name, decl.params.len(), false));
        self.at(decl.span);
        for p in &decl.params {
            let slot = self.add_local(p)?;
            self.cur().infos[slot as usize].start = 0;
        }
        for s in &decl.body { self.stmt(s)?; }
        self.emit(Op::Nil);
        self.emit(Op::Return); // implicit `return nil;`
        let proto = self.finish_fn();
        self.span = outer_span;
        self.at(decl.span);

        let protos = &mut self.cur().chunk.protos;
        if protos.len() >= u16::MAX as usize { return self.err("too many functions in one scope"); }
        protos.push(Rc::new(proto));
        let idx = (protos.len() - 1) as u16;
        self.emit(Op::Closure(idx));
        Ok(())
    }

    // ---------- expressions ----------

    fn expr(&mut self, e: &Expr) -> CResult {
        self.at(e.span);
        match &e.kind {
            ExprKind::Number(n) => self.emit_const(Value::Num(*n))?,
            ExprKind::Str(s) => {
                let g = self.heap.intern(s);
                self.emit_const(Value::Obj(g))?
            }
            ExprKind::Bool(true) => { self.emit(Op::True); }
            ExprKind::Bool(false) => { self.emit(Op::False); }
            ExprKind::Nil => { self.emit(Op::Nil); }
            ExprKind::Var(name) => self.emit_get(name)?,
            ExprKind::Interp(parts) => {
                for p in parts { self.expr(p)?; }
                self.at(e.span);
                self.emit(Op::Concat(parts.len() as u16));
            }
            ExprKind::List(items) => {
                if items.len() > u16::MAX as usize { return self.err("list literal too long"); }
                for it in items { self.expr(it)?; }
                self.at(e.span);
                self.emit(Op::MakeList(items.len() as u16));
            }
            ExprKind::Map(pairs) => {
                for (k, v) in pairs { self.expr(k)?; self.expr(v)?; }
                self.at(e.span);
                self.emit(Op::MakeMap(pairs.len() as u16));
            }
            ExprKind::Index(obj, idx) => {
                self.expr(obj)?;
                self.expr(idx)?;
                self.at(e.span);
                self.emit(Op::Index);
            }
            ExprKind::Call(callee, args) => {
                self.expr(callee)?;
                for a in args { self.expr(a)?; }
                self.at(e.span);
                self.emit(Op::Call(args.len() as u8));
            }
            ExprKind::Unary(op, operand) => {
                self.expr(operand)?;
                self.at(e.span);
                self.emit(match op { UnOp::Neg => Op::Neg, UnOp::Not => Op::Not });
            }
            ExprKind::Binary(op, l, r) => {
                self.expr(l)?;
                self.expr(r)?;
                self.at(e.span);
                self.emit(binop(*op));
            }
            ExprKind::Logical { is_and, left, right } => {
                self.expr(left)?;
                self.at(e.span);
                let jump = self.emit(if *is_and { Op::JumpIfFalseKeep(0) } else { Op::JumpIfTrueKeep(0) });
                self.emit(Op::Pop);
                self.expr(right)?;
                let end = self.here();
                self.patch(jump, end);
            }
            ExprKind::Func(decl) => self.function(decl)?,
        }
        Ok(())
    }
}

fn binop(op: BinOp) -> Op {
    match op {
        BinOp::Add => Op::Add, BinOp::Sub => Op::Sub, BinOp::Mul => Op::Mul,
        BinOp::Div => Op::Div, BinOp::Mod => Op::Mod, BinOp::Eq => Op::Eq,
        BinOp::Ne => Op::Ne, BinOp::Lt => Op::Lt, BinOp::Le => Op::Le,
        BinOp::Gt => Op::Gt, BinOp::Ge => Op::Ge,
    }
}
