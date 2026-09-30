//! Runtime values and the garbage-collected heap.
//!
//! Small values (nil, booleans, numbers, strings, native functions) live
//! directly inside `Value`. Lists, maps, closures and upvalues live on the
//! heap and are referred to by a `Gc` handle, which is just an index into
//! `Heap::objects`. The collector is a classic mark-and-sweep:
//!
//!   1. mark: starting from the roots (the VM stack, globals, call frames,
//!      open upvalues) follow every handle and mark what's reachable;
//!   2. sweep: free every object that wasn't marked and reuse its slot.
//!
//! Because reachability is traced rather than counted, cycles (a list that
//! contains itself, a closure stored in a variable it captures) are
//! collected correctly.
//!
//! Strings are heap objects too, and they're *interned*: each distinct
//! string exists exactly once. That makes `Value` a plain 16-byte `Copy`
//! type with no reference counts to update, makes string equality a single
//! integer comparison, and lets maps hash a 4-byte handle instead of the
//! whole string. (Lua uses the same design.)

use crate::bytecode::Proto;
use crate::natives::NativeDef;
use std::collections::HashMap;
use std::fmt::Write;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Gc(pub u32);

#[derive(Clone, Copy)]
pub enum Value {
    Nil,
    Bool(bool),
    Num(f64),
    Native(&'static NativeDef),
    Obj(Gc),
}

impl Value {
    #[inline]
    pub fn truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }
}

pub struct Closure {
    pub proto: Rc<Proto>,
    pub upvalues: Vec<Gc>,
}

/// A variable captured by a closure. While the function that owns the
/// variable is still running, the upvalue points at its stack slot ("open").
/// When that function returns, the value is moved into the upvalue ("closed").
pub enum Upvalue {
    Open(usize),
    Closed(Value),
}

pub enum Obj {
    Str(Rc<str>),
    List(Vec<Value>),
    Map(RMap),
    Closure(Closure),
    Upvalue(Upvalue),
}

// ------------------------------------------------------------------ maps

/// Map keys are restricted to values with a stable identity. Because
/// strings are interned, a string key is just its handle.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Nil,
    Bool(bool),
    Num(u64),
    Str(Gc),
}

impl Key {
    pub fn from_value(heap: &Heap, v: &Value) -> Result<Key, String> {
        match v {
            Value::Nil => Ok(Key::Nil),
            Value::Bool(b) => Ok(Key::Bool(*b)),
            Value::Num(n) if n.is_nan() => Err("NaN can't be used as a map key".into()),
            Value::Num(n) => Ok(Key::Num(if *n == 0.0 { 0f64.to_bits() } else { n.to_bits() })),
            Value::Obj(g) if matches!(heap.get(*g), Obj::Str(_)) => Ok(Key::Str(*g)),
            _ => Err("map keys must be strings, numbers, booleans or nil".into()),
        }
    }
    pub fn to_value(&self) -> Value {
        match self {
            Key::Nil => Value::Nil,
            Key::Bool(b) => Value::Bool(*b),
            Key::Num(bits) => Value::Num(f64::from_bits(*bits)),
            Key::Str(g) => Value::Obj(*g),
        }
    }
}

/// A hash map that remembers insertion order, so printing and iterating
/// a map is predictable.
#[derive(Default)]
pub struct RMap {
    index: HashMap<Key, usize>,
    pub entries: Vec<(Key, Value)>,
}

impl RMap {
    pub fn get(&self, k: &Key) -> Option<&Value> {
        self.index.get(k).map(|&i| &self.entries[i].1)
    }
    pub fn insert(&mut self, k: Key, v: Value) {
        match self.index.get(&k) {
            Some(&i) => self.entries[i].1 = v,
            None => {
                self.index.insert(k.clone(), self.entries.len());
                self.entries.push((k, v));
            }
        }
    }
    pub fn remove(&mut self, k: &Key) -> Option<Value> {
        let i = self.index.remove(k)?;
        let (_, v) = self.entries.remove(i);
        for (_, idx) in self.index.iter_mut() {
            if *idx > i { *idx -= 1; }
        }
        Some(v)
    }
    pub fn len(&self) -> usize { self.entries.len() }
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
}

// ------------------------------------------------------------------ heap

pub struct Heap {
    objects: Vec<Option<Obj>>,
    /// The intern table: every live string, by content.
    strings: HashMap<Rc<str>, Gc>,
    marks: Vec<bool>,
    free: Vec<u32>,
    live: usize,
    /// run a collection once this many objects are live
    pub next_gc: usize,
    pub collections: u64,
    pub total_freed: u64,
    pub force_gc: bool,
}

const MIN_GC_THRESHOLD: usize = 2048;

impl Default for Heap {
    fn default() -> Self { Self::new() }
}

impl Heap {
    pub fn new() -> Self {
        Heap {
            objects: Vec::new(),
            strings: HashMap::new(),
            marks: Vec::new(),
            free: Vec::new(),
            live: 0,
            next_gc: MIN_GC_THRESHOLD,
            collections: 0,
            total_freed: 0,
            force_gc: false,
        }
    }

    pub fn alloc(&mut self, obj: Obj) -> Gc {
        self.live += 1;
        if let Some(i) = self.free.pop() {
            self.objects[i as usize] = Some(obj);
            Gc(i)
        } else {
            self.objects.push(Some(obj));
            self.marks.push(false);
            Gc((self.objects.len() - 1) as u32)
        }
    }

    pub fn live(&self) -> usize { self.live }

    pub fn should_collect(&self) -> bool {
        self.force_gc || self.live >= self.next_gc
    }

    #[inline]
    pub fn get(&self, g: Gc) -> &Obj {
        self.objects[g.0 as usize].as_ref().expect("use of freed object")
    }
    #[inline]
    pub fn get_mut(&mut self, g: Gc) -> &mut Obj {
        self.objects[g.0 as usize].as_mut().expect("use of freed object")
    }

    /// The one heap string with this content, created if needed.
    pub fn intern(&mut self, s: &str) -> Gc {
        if let Some(&g) = self.strings.get(s) { return g; }
        let rc: Rc<str> = Rc::from(s);
        let g = self.alloc(Obj::Str(rc.clone()));
        self.strings.insert(rc, g);
        g
    }

    pub fn new_str(&mut self, s: &str) -> Value {
        Value::Obj(self.intern(s))
    }

    /// The text of a value if it's a string.
    #[inline]
    pub fn as_str(&self, v: &Value) -> Option<&str> {
        match v {
            Value::Obj(g) => match self.get(*g) { Obj::Str(s) => Some(s), _ => None },
            _ => None,
        }
    }

    pub fn new_list(&mut self, items: Vec<Value>) -> Value {
        Value::Obj(self.alloc(Obj::List(items)))
    }
    pub fn new_map(&mut self, map: RMap) -> Value {
        Value::Obj(self.alloc(Obj::Map(map)))
    }

    /// Mark everything reachable from the given roots, then free the rest.
    pub fn collect<'a>(&mut self, roots: impl Iterator<Item = &'a Value>, gc_roots: impl Iterator<Item = Gc>) {
        let mut work: Vec<Gc> = Vec::new();
        for v in roots {
            if let Value::Obj(g) = v { work.push(*g); }
        }
        work.extend(gc_roots);

        while let Some(g) = work.pop() {
            let i = g.0 as usize;
            if self.marks[i] { continue; }
            self.marks[i] = true;
            match self.objects[i].as_ref().unwrap() {
                Obj::List(items) => {
                    for v in items { if let Value::Obj(c) = v { work.push(*c); } }
                }
                Obj::Map(m) => {
                    for (k, v) in &m.entries {
                        if let Key::Str(c) = k { work.push(*c); }
                        if let Value::Obj(c) = v { work.push(*c); }
                    }
                }
                Obj::Str(_) => {}
                Obj::Closure(c) => work.extend(c.upvalues.iter().copied()),
                Obj::Upvalue(Upvalue::Closed(Value::Obj(c))) => work.push(*c),
                Obj::Upvalue(_) => {}
            }
        }

        // the intern table holds strings weakly: drop entries for dead strings
        let marks = &self.marks;
        self.strings.retain(|_, g| marks[g.0 as usize]);

        let mut freed = 0;
        for i in 0..self.objects.len() {
            if self.marks[i] {
                self.marks[i] = false;
            } else if self.objects[i].is_some() {
                self.objects[i] = None;
                self.free.push(i as u32);
                freed += 1;
            }
        }
        self.live -= freed;
        self.total_freed += freed as u64;
        self.collections += 1;
        self.force_gc = false;
        self.next_gc = (self.live * 2).max(MIN_GC_THRESHOLD);
    }

    // ---------- inspecting values ----------

    pub fn type_name(&self, v: &Value) -> &'static str {
        match v {
            Value::Nil => "nil",
            Value::Bool(_) => "bool",
            Value::Num(_) => "number",
            Value::Native(_) => "function",
            Value::Obj(g) => match self.get(*g) {
                Obj::Str(_) => "string",
                Obj::List(_) => "list",
                Obj::Map(_) => "map",
                Obj::Closure(_) => "function",
                Obj::Upvalue(_) => "upvalue",
            },
        }
    }

    pub fn equals(&self, a: &Value, b: &Value) -> bool {
        self.equals_depth(a, b, 0)
    }

    fn equals_depth(&self, a: &Value, b: &Value, depth: usize) -> bool {
        match (a, b) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(x), Value::Bool(y)) => x == y,
            (Value::Num(x), Value::Num(y)) => x == y,
            (Value::Native(x), Value::Native(y)) => std::ptr::eq(*x, *y),
            (Value::Obj(x), Value::Obj(y)) => {
                if x == y { return true; }
                if depth > 64 { return false; }
                match (self.get(*x), self.get(*y)) {
                    (Obj::List(p), Obj::List(q)) => {
                        p.len() == q.len() && p.iter().zip(q).all(|(u, v)| self.equals_depth(u, v, depth + 1))
                    }
                    (Obj::Map(p), Obj::Map(q)) => {
                        p.len() == q.len()
                            && p.entries.iter().all(|(k, v)| q.get(k).is_some_and(|w| self.equals_depth(v, w, depth + 1)))
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// How a value prints: strings are shown raw at the top level.
    pub fn display(&self, v: &Value) -> String {
        let mut out = String::new();
        self.write_value(v, &mut out, &mut Vec::new(), false);
        out
    }

    /// How a value looks in a debugger or inside a list: strings get quotes.
    pub fn repr(&self, v: &Value) -> String {
        let mut out = String::new();
        self.write_value(v, &mut out, &mut Vec::new(), true);
        out
    }

    fn write_value(&self, v: &Value, out: &mut String, seen: &mut Vec<Gc>, quoted: bool) {
        match v {
            Value::Nil => out.push_str("nil"),
            Value::Bool(b) => { let _ = write!(out, "{}", b); }
            Value::Num(n) => out.push_str(&fmt_num(*n)),
            Value::Native(n) => { let _ = write!(out, "<builtin {}>", n.name); }
            Value::Obj(g) => {
                if seen.contains(g) {
                    out.push_str("<cycle>");
                    return;
                }
                match self.get(*g) {
                    Obj::Str(s) if quoted => { let _ = write!(out, "{:?}", &**s); }
                    Obj::Str(s) => out.push_str(s),
                    Obj::List(items) => {
                        seen.push(*g);
                        out.push('[');
                        for (i, item) in items.iter().enumerate() {
                            if i > 0 { out.push_str(", "); }
                            self.write_value(item, out, seen, true);
                        }
                        out.push(']');
                        seen.pop();
                    }
                    Obj::Map(m) => {
                        seen.push(*g);
                        out.push('{');
                        for (i, (k, val)) in m.entries.iter().enumerate() {
                            if i > 0 { out.push(','); }
                            out.push(' ');
                            match self.as_str(&k.to_value()) {
                                Some(s) if is_ident(s) => out.push_str(s),
                                _ => self.write_value(&k.to_value(), out, seen, true),
                            }
                            out.push_str(": ");
                            self.write_value(val, out, seen, true);
                        }
                        out.push_str(if m.is_empty() { "}" } else { " }" });
                        seen.pop();
                    }
                    Obj::Closure(c) => { let _ = write!(out, "<fn {}>", c.proto.name); }
                    Obj::Upvalue(_) => out.push_str("<upvalue>"),
                }
            }
        }
    }
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_alphabetic() || c == '_') && chars.all(|c| c.is_alphanumeric() || c == '_')
}

pub fn fmt_num(n: f64) -> String {
    if n.is_nan() {
        "nan".into()
    } else if n.is_infinite() {
        if n > 0.0 { "inf".into() } else { "-inf".into() }
    } else if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        // shortest representation that round-trips, like most languages
        format!("{}", n)
    }
}
