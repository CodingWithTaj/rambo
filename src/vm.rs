//! The virtual machine.
//!
//! One value stack is shared by every call. A call frame is just a window
//! into that stack (`base`) plus an instruction pointer. The VM is
//! *resumable*: `run(budget)` executes at most `budget` instructions and can
//! be called again to continue, which is what the step-through debugger uses.

use crate::bytecode::*;
use crate::compiler::Globals;
use crate::natives::{self, Ctx, NATIVES};
use crate::value::*;
use std::rc::Rc;

pub const MAX_CALL_DEPTH: usize = 1000;

#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub msg: String,
    /// (function name, line, column), innermost call first; prelude frames excluded
    pub trace: Vec<(String, u32, u32)>,
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.msg)?;
        for (name, line, _) in &self.trace {
            write!(f, "\n  at {} (line {})", name, line)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    /// Out of budget; call `run` again to continue.
    Paused,
    Finished,
}

pub struct CallFrame {
    pub closure: Gc,
    pub proto: Rc<Proto>,
    pub ip: usize,
    pub base: usize,
}

pub struct Vm {
    pub heap: Heap,
    pub globals: Globals,
    pub global_values: Vec<Option<Value>>,
    pub stack: Vec<Value>,
    pub frames: Vec<CallFrame>,
    open_upvalues: Vec<Gc>,
    /// Everything print() has written.
    pub output: String,
    /// Print output to stdout as it's produced (the command line runner).
    pub echo: bool,
    /// Stop after this many instructions in total (guards against infinite loops).
    pub step_limit: Option<u64>,
    pub steps: u64,
    rng: u64,
    /// Globals before this index are built-ins and prelude functions.
    user_globals_start: usize,
    /// String constants from every compiled function. Code can be re-run
    /// (functions are values), so these stay alive for the VM's lifetime.
    const_roots: Vec<Gc>,
}

impl Default for Vm {
    fn default() -> Self { Self::new() }
}

impl Vm {
    pub fn new() -> Self {
        let mut vm = Vm {
            heap: Heap::new(),
            globals: Globals::default(),
            global_values: Vec::new(),
            stack: Vec::with_capacity(1024),
            frames: Vec::with_capacity(64),
            open_upvalues: Vec::new(),
            output: String::new(),
            echo: false,
            step_limit: None,
            steps: 0,
            rng: 0x853C49E6748FEA9B,
            user_globals_start: 0,
            const_roots: Vec::new(),
        };
        for def in NATIVES.iter() {
            let slot = vm.globals.slot(def.name).unwrap() as usize;
            vm.set_global(slot, Value::Native(def));
        }
        let proto = crate::compile_with(natives::PRELUDE, &mut vm.globals, &mut vm.heap, true).expect("prelude compiles");
        vm.load_proto(proto);
        vm.run(u64::MAX).expect("prelude runs");
        vm.steps = 0;
        vm.user_globals_start = vm.globals.names.len();
        vm
    }

    fn set_global(&mut self, slot: usize, v: Value) {
        if self.global_values.len() <= slot { self.global_values.resize(slot + 1, None); }
        self.global_values[slot] = Some(v);
    }

    /// Compile source and prepare to run it (then call `run`).
    pub fn load(&mut self, src: &str) -> Result<(), crate::RamboError> {
        let proto = crate::compile_with(src, &mut self.globals, &mut self.heap, false)?;
        self.load_proto(proto);
        Ok(())
    }

    fn root_constants(&mut self, proto: &Proto) {
        for c in &proto.chunk.consts {
            if let Value::Obj(g) = c { self.const_roots.push(*g); }
        }
        for child in &proto.chunk.protos { self.root_constants(child); }
    }

    pub fn load_proto(&mut self, proto: Rc<Proto>) {
        self.root_constants(&proto);
        self.stack.clear();
        self.frames.clear();
        self.open_upvalues.clear();
        let closure = self.heap.alloc(Obj::Closure(Closure { proto: proto.clone(), upvalues: vec![] }));
        self.stack.push(Value::Obj(closure));
        self.frames.push(CallFrame { closure, proto, ip: 0, base: 0 });
    }

    pub fn is_running(&self) -> bool { !self.frames.is_empty() }

    /// The (line, column) of the next instruction to run.
    pub fn position(&self) -> Option<(u32, u32)> {
        self.frames.last().map(|f| f.proto.span_at(f.ip))
    }

    pub fn flush(&mut self) {
        if self.echo && !self.output.is_empty() {
            print!("{}", self.output);
            self.output.clear();
        }
    }

    fn collect_garbage(&mut self) {
        let roots = self.stack.iter().chain(self.global_values.iter().flatten());
        let gc_roots = self.frames.iter().map(|f| f.closure)
            .chain(self.open_upvalues.iter().copied())
            .chain(self.const_roots.iter().copied());
        self.heap.collect(roots, gc_roots);
    }

    /// Execute up to `budget` instructions.
    pub fn run(&mut self, budget: u64) -> Result<Status, RuntimeError> {
        let result = self.execute(budget);
        self.flush();
        match result {
            Ok(status) => Ok(status),
            Err(mut msg) => {
                if let Some(f) = self.frames.last() {
                    if f.proto.is_prelude { msg = format!("inside {}(): {}", f.proto.name, msg); }
                }
                let trace = self.frames.iter().rev()
                    .filter(|f| !f.proto.is_prelude)
                    .map(|f| {
                        let (line, col) = f.proto.span_at(f.ip.saturating_sub(1));
                        (f.proto.name.clone(), line, col)
                    })
                    .collect();
                self.frames.clear();
                self.stack.clear();
                self.open_upvalues.clear();
                Err(RuntimeError { msg, trace })
            }
        }
    }

    // ---------- upvalues ----------

    fn capture_upvalue(&mut self, slot: usize) -> Gc {
        // open_upvalues is sorted by slot so closures share one upvalue per variable
        for &g in self.open_upvalues.iter().rev() {
            if let Obj::Upvalue(Upvalue::Open(s)) = self.heap.get(g) {
                if *s == slot { return g; }
                if *s < slot { break; }
            }
        }
        let g = self.heap.alloc(Obj::Upvalue(Upvalue::Open(slot)));
        let pos = self.open_upvalues.iter()
            .position(|&u| matches!(self.heap.get(u), Obj::Upvalue(Upvalue::Open(s)) if *s > slot))
            .unwrap_or(self.open_upvalues.len());
        self.open_upvalues.insert(pos, g);
        g
    }

    /// Close every open upvalue that points at `from` or above in the stack.
    fn close_upvalues(&mut self, from: usize) {
        while let Some(&g) = self.open_upvalues.last() {
            let slot = match self.heap.get(g) { Obj::Upvalue(Upvalue::Open(s)) => *s, _ => unreachable!() };
            if slot < from { break; }
            let v = self.stack[slot];
            *self.heap.get_mut(g) = Obj::Upvalue(Upvalue::Closed(v));
            self.open_upvalues.pop();
        }
    }

    fn read_upvalue(&self, g: Gc) -> Value {
        match self.heap.get(g) {
            Obj::Upvalue(Upvalue::Open(slot)) => self.stack[*slot],
            Obj::Upvalue(Upvalue::Closed(v)) => *v,
            _ => unreachable!(),
        }
    }

    fn write_upvalue(&mut self, g: Gc, v: Value) {
        match self.heap.get_mut(g) {
            Obj::Upvalue(Upvalue::Open(slot)) => { let s = *slot; self.stack[s] = v; }
            Obj::Upvalue(Upvalue::Closed(c)) => *c = v,
            _ => unreachable!(),
        }
    }

    fn upvalue_handle(&self, closure: Gc, i: u16) -> Gc {
        match self.heap.get(closure) {
            Obj::Closure(c) => c.upvalues[i as usize],
            _ => unreachable!(),
        }
    }

    // ---------- the main loop ----------

    #[inline(always)]
    fn pop(&mut self) -> Value {
        // SAFETY: the compiler only emits code that pops values it pushed,
        // and Value is Copy, so shrinking the length is all a pop needs.
        debug_assert!(!self.stack.is_empty());
        unsafe {
            let n = self.stack.len() - 1;
            self.stack.set_len(n);
            *self.stack.as_ptr().add(n)
        }
    }

    fn type_name(&self, v: &Value) -> &'static str { self.heap.type_name(v) }

    fn execute(&mut self, budget: u64) -> Result<Status, String> {
        if self.frames.is_empty() { return Ok(Status::Finished); }
        let mut frame_idx = self.frames.len() - 1;
        let mut proto = self.frames[frame_idx].proto.clone();
        let mut ip = self.frames[frame_idx].ip;
        let mut base = self.frames[frame_idx].base;
        // One countdown covers both the caller's budget and the step limit,
        // so the hot loop checks a single local counter per instruction.
        let limit_left = self.step_limit.map(|l| l.saturating_sub(self.steps)).unwrap_or(u64::MAX);
        let allowed = budget.min(limit_left);
        let mut remaining = allowed;
        // Direct pointer to the current function's instructions, refreshed on
        // every call and return. Saves several pointer hops per instruction.
        let mut code: *const Op = proto.chunk.code.as_ptr();

        macro_rules! save_ip { () => {{
            self.frames[frame_idx].ip = ip;
            self.steps += allowed - remaining;
            #[allow(unused_assignments)] { remaining = allowed; }
        }}; }
        macro_rules! fail { ($($arg:tt)*) => {{ save_ip!(); return Err(format!($($arg)*)); }}; }
        // Garbage collection only happens at "safe points": backward jumps and
        // calls. Straight-line code can only allocate a bounded amount, so
        // checking here is enough, and it keeps the check out of every instruction.
        macro_rules! safe_point { () => {
            if self.heap.should_collect() { self.collect_garbage(); }
        }; }

        loop {
            if remaining == 0 {
                save_ip!();
                if let Some(limit) = self.step_limit {
                    if self.steps >= limit && allowed < budget {
                        return Err(format!("stopped after {} steps (is there an infinite loop?)", limit));
                    }
                }
                return Ok(Status::Paused);
            }
            remaining -= 1;

            // SAFETY: every function ends in RETURN and jumps only target
            // instructions in the same function, so ip stays in bounds.
            let op = unsafe { *code.add(ip) };
            ip += 1;

            match op {
                Op::Const(i) => self.stack.push(proto.chunk.consts[i as usize]),
                Op::Nil => self.stack.push(Value::Nil),
                Op::True => self.stack.push(Value::Bool(true)),
                Op::False => self.stack.push(Value::Bool(false)),
                Op::Pop => { self.stack.pop(); }
                Op::Dup2 => {
                    let n = self.stack.len();
                    let (a, b) = (self.stack[n - 2], self.stack[n - 1]);
                    self.stack.push(a);
                    self.stack.push(b);
                }

                Op::GetLocal(s) => { let v = self.stack[base + s as usize]; self.stack.push(v); }
                Op::SetLocal(s) => { let v = self.pop(); self.stack[base + s as usize] = v; }
                Op::GetUpvalue(i) => {
                    let g = self.upvalue_handle(self.frames[frame_idx].closure, i);
                    let v = self.read_upvalue(g);
                    self.stack.push(v);
                }
                Op::SetUpvalue(i) => {
                    let g = self.upvalue_handle(self.frames[frame_idx].closure, i);
                    let v = self.pop();
                    self.write_upvalue(g, v);
                }
                Op::CloseUpvalue => {
                    let top = self.stack.len() - 1;
                    self.close_upvalues(top);
                    self.stack.pop();
                }

                Op::GetGlobal(g) => match self.global_values.get(g as usize) {
                    Some(Some(v)) => { let v = *v; self.stack.push(v); }
                    _ => fail!("'{}' is not defined", self.globals.names[g as usize]),
                },
                Op::SetGlobal(g) => {
                    let v = self.pop();
                    match self.global_values.get_mut(g as usize) {
                        Some(slot @ Some(_)) => *slot = Some(v),
                        _ => fail!("'{}' is not defined (declare it first with let)", self.globals.names[g as usize]),
                    }
                }
                Op::DefineGlobal(g) => { let v = self.pop(); self.set_global(g as usize, v); }

                Op::Add | Op::Sub | Op::Mul | Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::Mod
                    if matches!(self.stack[self.stack.len() - 2..], [Value::Num(_), Value::Num(_)]) =>
                {
                    // Fast path for the common case: both operands are numbers.
                    let n = self.stack.len();
                    let (x, y) = match (&self.stack[n - 2], &self.stack[n - 1]) {
                        (Value::Num(x), Value::Num(y)) => (*x, *y),
                        _ => unreachable!(),
                    };
                    let r = match op {
                        Op::Add => Value::Num(x + y),
                        Op::Sub => Value::Num(x - y),
                        Op::Mul => Value::Num(x * y),
                        Op::Lt => Value::Bool(x < y),
                        Op::Le => Value::Bool(x <= y),
                        Op::Gt => Value::Bool(x > y),
                        Op::Ge => Value::Bool(x >= y),
                        _ => {
                            if y == 0.0 { fail!("division by zero"); }
                            Value::Num(modulo(x, y))
                        }
                    };
                    self.stack.pop();
                    self.stack[n - 2] = r;
                }
                Op::Add => {
                    let (b, a) = (self.pop(), self.pop());
                    let r = match (&a, &b) {
                        (Value::Num(x), Value::Num(y)) => Value::Num(x + y),
                        _ if self.heap.as_str(&a).is_some() || self.heap.as_str(&b).is_some() => {
                            // string + anything joins their printed forms
                            let s = format!("{}{}", self.heap.display(&a), self.heap.display(&b));
                            self.heap.new_str(&s)
                        }
                        (Value::Obj(x), Value::Obj(y)) => match (self.heap.get(*x), self.heap.get(*y)) {
                            (Obj::List(p), Obj::List(q)) => {
                                let mut items = p.clone();
                                items.extend_from_slice(q);
                                self.heap.new_list(items)
                            }
                            _ => fail!("can't add {} and {}", self.type_name(&a), self.type_name(&b)),
                        },
                        _ => fail!("can't add {} and {}", self.type_name(&a), self.type_name(&b)),
                    };
                    self.stack.push(r);
                }
                Op::Mul => {
                    let (b, a) = (self.pop(), self.pop());
                    let repeat = match (&a, &b) {
                        (Value::Num(x), Value::Num(y)) => { self.stack.push(Value::Num(x * y)); continue; }
                        (s, Value::Num(n)) | (Value::Num(n), s) => self.heap.as_str(s).map(|s| (s.to_string(), *n)),
                        _ => None,
                    };
                    let r = match repeat {
                        Some((s, n)) => {
                            if n < 0.0 || n.fract() != 0.0 || n * s.len() as f64 > 1e8 {
                                fail!("a string can only be repeated a whole, non-negative number of times");
                            }
                            self.heap.new_str(&s.repeat(n as usize))
                        }
                        None => fail!("can't multiply {} and {}", self.type_name(&a), self.type_name(&b)),
                    };
                    self.stack.push(r);
                }
                Op::Sub | Op::Div | Op::Mod => {
                    let (b, a) = (self.pop(), self.pop());
                    let (x, y) = match (&a, &b) {
                        (Value::Num(x), Value::Num(y)) => (*x, *y),
                        _ => {
                            let verb = match op { Op::Sub => "subtract", Op::Div => "divide", _ => "take the remainder of" };
                            fail!("can't {} {} and {}", verb, self.type_name(&a), self.type_name(&b));
                        }
                    };
                    let r = match op {
                        Op::Sub => x - y,
                        _ if y == 0.0 => fail!("division by zero"),
                        Op::Div => x / y,
                        _ => modulo(x, y),
                    };
                    self.stack.push(Value::Num(r));
                }
                Op::Eq => { let (b, a) = (self.pop(), self.pop()); let r = self.heap.equals(&a, &b); self.stack.push(Value::Bool(r)); }
                Op::Ne => { let (b, a) = (self.pop(), self.pop()); let r = !self.heap.equals(&a, &b); self.stack.push(Value::Bool(r)); }
                Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                    let (b, a) = (self.pop(), self.pop());
                    let ord = match (&a, &b, self.heap.as_str(&a), self.heap.as_str(&b)) {
                        (Value::Num(x), Value::Num(y), _, _) => x.partial_cmp(y),
                        (_, _, Some(x), Some(y)) => Some(x.cmp(y)),
                        _ => fail!("can't compare {} with {}", self.type_name(&a), self.type_name(&b)),
                    };
                    use std::cmp::Ordering::*;
                    let r = match (op, ord) {
                        (_, None) => false,
                        (Op::Lt, Some(o)) => o == Less,
                        (Op::Le, Some(o)) => o != Greater,
                        (Op::Gt, Some(o)) => o == Greater,
                        (_, Some(o)) => o != Less,
                    };
                    self.stack.push(Value::Bool(r));
                }
                Op::Neg => match self.pop() {
                    Value::Num(n) => self.stack.push(Value::Num(-n)),
                    v => fail!("can't negate {}", self.type_name(&v)),
                },
                Op::Not => { let v = self.pop(); self.stack.push(Value::Bool(!v.truthy())); }

                Op::Jump(t) => {
                    if (t as usize) < ip { safe_point!(); }
                    ip = t as usize;
                }
                Op::JumpIfFalse(t) => { if !self.pop().truthy() { ip = t as usize; } }
                Op::JumpIfNotLt(t) | Op::JumpIfNotLe(t) | Op::JumpIfNotGt(t) | Op::JumpIfNotGe(t) => {
                    let n = self.stack.len();
                    let holds = match (&self.stack[n - 2], &self.stack[n - 1]) {
                        (Value::Num(x), Value::Num(y)) => {
                            let (x, y) = (*x, *y);
                            // SAFETY: both are numbers, which need no drop
                            unsafe { self.stack.set_len(n - 2); }
                            match op {
                                Op::JumpIfNotLt(_) => x < y,
                                Op::JumpIfNotLe(_) => x <= y,
                                Op::JumpIfNotGt(_) => x > y,
                                _ => x >= y,
                            }
                        }
                        _ => {
                            let (b, a) = (self.pop(), self.pop());
                            let ord = match (self.heap.as_str(&a), self.heap.as_str(&b)) {
                                (Some(x), Some(y)) => x.cmp(y),
                                _ => fail!("can't compare {} with {}", self.type_name(&a), self.type_name(&b)),
                            };
                            use std::cmp::Ordering::*;
                            match op {
                                Op::JumpIfNotLt(_) => ord == Less,
                                Op::JumpIfNotLe(_) => ord != Greater,
                                Op::JumpIfNotGt(_) => ord == Greater,
                                _ => ord != Less,
                            }
                        }
                    };
                    if !holds { ip = t as usize; }
                }
                Op::JumpIfFalseKeep(t) => { if !self.stack.last().unwrap().truthy() { ip = t as usize; } }
                Op::JumpIfTrueKeep(t) => { if self.stack.last().unwrap().truthy() { ip = t as usize; } }

                Op::Closure(i) => {
                    let child = proto.chunk.protos[i as usize].clone();
                    let parent = self.frames[frame_idx].closure;
                    let mut upvalues = Vec::with_capacity(child.upvalues.len());
                    for &(is_local, idx) in &child.upvalues {
                        upvalues.push(if is_local { self.capture_upvalue(base + idx as usize) } else { self.upvalue_handle(parent, idx) });
                    }
                    let g = self.heap.alloc(Obj::Closure(Closure { proto: child, upvalues }));
                    self.stack.push(Value::Obj(g));
                }

                Op::Call(argc) => {
                    let argc = argc as usize;
                    let callee_at = self.stack.len() - 1 - argc;
                    match &self.stack[callee_at] {
                        Value::Obj(g) => {
                            let g = *g;
                            let callee = match self.heap.get(g) {
                                Obj::Closure(c) => c.proto.clone(),
                                _ => fail!("can only call functions, not a {}", self.type_name(&self.stack[callee_at])),
                            };
                            if callee.arity != argc {
                                fail!("{}() expects {} argument{}, got {}", callee.name, callee.arity, if callee.arity == 1 { "" } else { "s" }, argc);
                            }
                            if self.frames.len() >= MAX_CALL_DEPTH {
                                fail!("stack overflow: more than {} nested calls (is a function recursing forever?)", MAX_CALL_DEPTH);
                            }
                            safe_point!();
                            self.frames[frame_idx].ip = ip;
                            self.frames.push(CallFrame { closure: g, proto: callee.clone(), ip: 0, base: callee_at });
                            frame_idx += 1;
                            proto = callee;
                            code = proto.chunk.code.as_ptr();
                            ip = 0;
                            base = callee_at;
                        }
                        Value::Native(def) => {
                            let def = *def;
                            if argc < def.min_args || argc > def.max_args {
                                let expected = if def.min_args == def.max_args { format!("{}", def.min_args) }
                                    else if def.max_args == usize::MAX { format!("at least {}", def.min_args) }
                                    else { format!("{} to {}", def.min_args, def.max_args) };
                                fail!("{}() expects {} argument{}, got {}", def.name, expected, if def.max_args == 1 { "" } else { "s" }, argc);
                            }
                            let mut cx = Ctx { heap: &mut self.heap, out: &mut self.output, rng: &mut self.rng };
                            let result = (def.func)(&mut cx, &self.stack[callee_at + 1..]);
                            match result {
                                Ok(v) => {
                                    self.stack.truncate(callee_at);
                                    self.stack.push(v);
                                    if self.echo && self.output.len() > 8192 { self.flush(); }
                                    safe_point!();
                                }
                                Err(e) if def.name == "error" || def.name == "assert" => fail!("{}", e),
                                Err(e) => fail!("{}(): {}", def.name, e),
                            }
                        }
                        other => fail!("can only call functions, not {}", self.type_name(&other.clone())),
                    }
                }
                Op::Return => {
                    let result = self.pop();
                    self.close_upvalues(base);
                    self.frames.pop();
                    self.stack.truncate(base);
                    if self.frames.is_empty() {
                        self.steps += allowed - remaining;
                        return Ok(Status::Finished);
                    }
                    self.stack.push(result);
                    frame_idx -= 1;
                    let f = &self.frames[frame_idx];
                    proto = f.proto.clone();
                    code = proto.chunk.code.as_ptr();
                    ip = f.ip;
                    base = f.base;
                }

                Op::MakeList(n) => {
                    let items = self.stack.split_off(self.stack.len() - n as usize);
                    let v = self.heap.new_list(items);
                    self.stack.push(v);
                }
                Op::MakeMap(n) => {
                    let flat = self.stack.split_off(self.stack.len() - 2 * n as usize);
                    let mut map = RMap::default();
                    for pair in flat.chunks(2) {
                        match Key::from_value(&self.heap, &pair[0]) {
                            Ok(k) => map.insert(k, pair[1]),
                            Err(e) => fail!("{}", e),
                        }
                    }
                    let v = self.heap.new_map(map);
                    self.stack.push(v);
                }
                Op::Concat(n) => {
                    let parts = self.stack.split_off(self.stack.len() - n as usize);
                    let mut s = String::new();
                    for p in &parts { s.push_str(&self.heap.display(p)); }
                    let v = self.heap.new_str(&s);
                    self.stack.push(v);
                }
                Op::Index => {
                    // fast path: list[whole number]
                    let n = self.stack.len();
                    if let (Value::Obj(g), Value::Num(i)) = (self.stack[n - 2], self.stack[n - 1]) {
                        if let Obj::List(items) = self.heap.get(g) {
                            let at = i as usize;
                            if at as f64 == i && at < items.len() {
                                let v = items[at];
                                self.stack.pop();
                                self.stack[n - 2] = v;
                                continue;
                            }
                        }
                    }
                    let (idx, obj) = (self.pop(), self.pop());
                    let v = match self.index(&obj, &idx) { Ok(v) => v, Err(e) => fail!("{}", e) };
                    self.stack.push(v);
                }
                Op::SetIndex => {
                    let (v, idx, obj) = (self.pop(), self.pop(), self.pop());
                    // fast path: list[whole number] = value
                    if let (Value::Obj(g), Value::Num(i)) = (obj, idx) {
                        if let Obj::List(items) = self.heap.get_mut(g) {
                            let at = i as usize;
                            if at as f64 == i && at < items.len() {
                                items[at] = v;
                                continue;
                            }
                        }
                    }
                    if let Err(e) = self.set_index(&obj, &idx, v) { fail!("{}", e); }
                }
                Op::IterPrep => {
                    let v = self.pop();
                    let prepared = match &v {
                        Value::Obj(g) => match self.heap.get(*g) {
                            Obj::Str(_) | Obj::List(_) => v,
                            Obj::Map(m) => {
                                let keys = m.entries.iter().map(|(k, _)| k.to_value()).collect();
                                self.heap.new_list(keys)
                            }
                            _ => fail!("can't loop over a {}", self.type_name(&v)),
                        },
                        _ => fail!("can't loop over a {} (expected a list, map or string)", self.type_name(&v)),
                    };
                    self.stack.push(prepared);
                }
                Op::Len => {
                    let v = self.pop();
                    let n = match &v {
                        Value::Obj(g) => match self.heap.get(*g) {
                            Obj::List(l) => l.len(),
                            Obj::Str(s) => s.chars().count(),
                            _ => 0,
                        },
                        _ => 0,
                    };
                    self.stack.push(Value::Num(n as f64));
                }
            }
        }
    }

    fn index(&mut self, obj: &Value, idx: &Value) -> Result<Value, String> {
        match obj {
            Value::Obj(g) => match self.heap.get(*g) {
                Obj::List(items) => {
                    let at = resolve_index(whole_index(&self.heap, idx)?, items.len())?;
                    Ok(items[at])
                }
                Obj::Map(m) => Ok(m.get(&Key::from_value(&self.heap, idx)?).copied().unwrap_or(Value::Nil)),
                Obj::Str(s) => {
                    let i = whole_index(&self.heap, idx)?;
                    let ch = s.chars().nth(resolve_index(i, s.chars().count())?).unwrap();
                    Ok(self.heap.new_str(ch.encode_utf8(&mut [0; 4])))
                }
                _ => Err(format!("can't index into a {}", self.type_name(obj))),
            },
            _ => Err(format!("can't index into {}", self.type_name(obj))),
        }
    }

    fn set_index(&mut self, obj: &Value, idx: &Value, v: Value) -> Result<(), String> {
        if let Value::Obj(g) = obj {
            let key = Key::from_value(&self.heap, idx);
            let index = whole_index(&self.heap, idx);
            match self.heap.get_mut(*g) {
                Obj::List(items) => {
                    let at = resolve_index(index?, items.len())?;
                    items[at] = v;
                    return Ok(());
                }
                Obj::Map(m) => {
                    m.insert(key?, v);
                    return Ok(());
                }
                Obj::Str(_) => return Err("strings can't be changed in place (build a new one instead)".into()),
                _ => {}
            }
        }
        Err(format!("can't set an index on {}", self.type_name(obj)))
    }

    // ---------- inspection, for the debugger ----------

    /// User-visible global variables (not built-ins or prelude functions).
    pub fn user_globals(&self) -> Vec<(String, Value)> {
        self.globals.names.iter().enumerate().skip(self.user_globals_start)
            .filter_map(|(i, name)| self.global_values.get(i).copied().flatten().map(|v| (name.to_string(), v)))
            .collect()
    }

    /// Captured variables visible to a frame: (name, value).
    pub fn frame_upvalues(&self, frame: &CallFrame) -> Vec<(String, Value)> {
        match self.heap.get(frame.closure) {
            Obj::Closure(c) => c.proto.upvalue_names.iter().zip(&c.upvalues)
                .map(|(n, g)| (n.clone(), self.read_upvalue(*g)))
                .collect(),
            _ => vec![],
        }
    }
}

/// Remainder with the sign of the divisor's absolute value (always >= 0 for
/// positive divisors). Whole numbers take an integer fast path, since
/// floating-point remainder is a slow library call on most CPUs.
#[inline]
fn modulo(x: f64, y: f64) -> f64 {
    let (xi, yi) = (x as i64, y as i64);
    if xi as f64 == x && yi as f64 == y && yi != 0 && xi.unsigned_abs() < (1 << 53) {
        xi.rem_euclid(yi) as f64
    } else {
        x.rem_euclid(y)
    }
}

fn whole_index(heap: &Heap, idx: &Value) -> Result<i64, String> {
    match idx {
        Value::Num(n) if n.fract() == 0.0 => Ok(*n as i64),
        Value::Num(n) => Err(format!("list index must be a whole number, got {}", fmt_num(*n))),
        _ => match heap.as_str(idx) {
            Some(s) => Err(format!("lists are indexed by number, not by \"{}\"", s)),
            None => Err("list index must be a number".into()),
        },
    }
}

/// Negative indexes count from the end: xs[-1] is the last item.
fn resolve_index(i: i64, len: usize) -> Result<usize, String> {
    let at = if i < 0 { len as i64 + i } else { i };
    if at < 0 || at >= len as i64 {
        return Err(format!("index {} is out of range (length {})", i, len));
    }
    Ok(at as usize)
}
