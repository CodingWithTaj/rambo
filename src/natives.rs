//! The standard library.
//!
//! Most built-ins are written in Rust (`NATIVES`). Higher-order helpers like
//! `map` and `filter` are written in Rambo itself (`PRELUDE`), which is
//! compiled and run when the VM starts. That keeps the VM simple: native
//! functions never have to call back into Rambo code.

use crate::value::{Heap, Key, Obj, RMap, Value};

pub struct Ctx<'a> {
    pub heap: &'a mut Heap,
    pub out: &'a mut String,
    pub rng: &'a mut u64,
}

pub type NativeFn = fn(&mut Ctx, &[Value]) -> Result<Value, String>;

pub struct NativeDef {
    pub name: &'static str,
    pub min_args: usize,
    /// usize::MAX = variadic
    pub max_args: usize,
    pub sig: &'static str,
    pub doc: &'static str,
    pub func: NativeFn,
}

macro_rules! native {
    ($name:literal, $min:expr, $max:expr, $sig:literal, $doc:literal, $f:expr) => {
        NativeDef { name: $name, min_args: $min, max_args: $max, sig: $sig, doc: $doc, func: $f }
    };
}

const ANY: usize = usize::MAX;

pub static NATIVES: &[NativeDef] = &[
    // ---- core
    native!("print", 0, ANY, "print(...values)", "Print values separated by spaces, followed by a newline.", print),
    native!("type", 1, 1, "type(value)", "The type of a value as a string: \"nil\", \"bool\", \"number\", \"string\", \"list\", \"map\" or \"function\".", type_of),
    native!("str", 1, 1, "str(value)", "Convert any value to its string form.", to_str),
    native!("num", 1, 1, "num(value)", "Convert a string to a number, or return nil if it isn't one.", to_num),
    native!("len", 1, 1, "len(collection)", "The number of items in a list or map, or characters in a string.", len),
    native!("assert", 1, 2, "assert(condition, message?)", "Stop with an error if the condition is falsy.", assert),
    native!("error", 1, 1, "error(message)", "Stop the program with a runtime error.", error),
    native!("clock", 0, 0, "clock()", "Seconds elapsed since the program started, with sub-millisecond precision.", clock),
    native!("gc", 0, 0, "gc()", "Ask the garbage collector to run before the next instruction.", gc),
    native!("memory", 0, 0, "memory()", "Garbage collector statistics: { live, collections, freed }.", memory),
    // ---- math
    native!("floor", 1, 1, "floor(x)", "Round down to a whole number.", |_, a| Ok(Value::Num(num(&a[0])?.floor()))),
    native!("ceil", 1, 1, "ceil(x)", "Round up to a whole number.", |_, a| Ok(Value::Num(num(&a[0])?.ceil()))),
    native!("round", 1, 1, "round(x)", "Round to the nearest whole number (halves round away from zero).", |_, a| Ok(Value::Num(num(&a[0])?.round()))),
    native!("abs", 1, 1, "abs(x)", "Absolute value.", |_, a| Ok(Value::Num(num(&a[0])?.abs()))),
    native!("sqrt", 1, 1, "sqrt(x)", "Square root.", |_, a| Ok(Value::Num(num(&a[0])?.sqrt()))),
    native!("pow", 2, 2, "pow(base, exponent)", "Raise base to a power.", |_, a| Ok(Value::Num(num(&a[0])?.powf(num(&a[1])?)))),
    native!("min", 2, 2, "min(a, b)", "The smaller of two numbers.", |_, a| Ok(Value::Num(num(&a[0])?.min(num(&a[1])?)))),
    native!("max", 2, 2, "max(a, b)", "The larger of two numbers.", |_, a| Ok(Value::Num(num(&a[0])?.max(num(&a[1])?)))),
    native!("random", 0, 0, "random()", "A random number from 0 (inclusive) to 1 (exclusive).", random),
    native!("random_int", 2, 2, "random_int(low, high)", "A random whole number from low to high, both inclusive.", random_int),
    native!("seed", 1, 1, "seed(n)", "Seed the random number generator, for repeatable results.", seed),
    // ---- lists
    native!("push", 2, 2, "push(list, value)", "Add a value to the end of a list.", push),
    native!("pop", 1, 1, "pop(list)", "Remove and return the last item of a list.", pop),
    native!("insert", 3, 3, "insert(list, index, value)", "Insert a value before the given index.", insert),
    native!("remove", 2, 2, "remove(collection, index_or_key)", "Remove an item from a list by index or from a map by key, returning it.", remove),
    native!("slice", 2, 3, "slice(list_or_string, start, end?)", "A copy of part of a list or string, from start up to (not including) end.", slice),
    native!("contains", 2, 2, "contains(collection, value)", "Whether a list has the value, a string has the substring, or a map has the key.", contains),
    native!("index_of", 2, 2, "index_of(list_or_string, value)", "Position of the first match, or nil if there is none.", index_of),
    native!("reverse", 1, 1, "reverse(list_or_string)", "A reversed copy.", reverse),
    native!("sort", 1, 1, "sort(list)", "A sorted copy of a list of numbers or a list of strings.", sort),
    native!("join", 2, 2, "join(list, separator)", "Join the items of a list into a string.", join),
    native!("range", 1, 3, "range(end) / range(start, end, step?)", "A list of numbers counting up (or down) to end, not including it.", range),
    // ---- strings
    native!("split", 2, 2, "split(string, separator)", "Split a string into a list. An empty separator splits into characters.", split),
    native!("upper", 1, 1, "upper(string)", "Uppercase copy.", |cx, a| { let s = text(cx.heap, &a[0])?.to_uppercase(); Ok(cx.heap.new_str(&s)) }),
    native!("lower", 1, 1, "lower(string)", "Lowercase copy.", |cx, a| { let s = text(cx.heap, &a[0])?.to_lowercase(); Ok(cx.heap.new_str(&s)) }),
    native!("trim", 1, 1, "trim(string)", "Copy without leading and trailing whitespace.", |cx, a| { let s = text(cx.heap, &a[0])?.trim().to_string(); Ok(cx.heap.new_str(&s)) }),
    native!("replace", 3, 3, "replace(string, from, to)", "Replace every occurrence of from with to.", replace),
    native!("starts_with", 2, 2, "starts_with(string, prefix)", "Whether the string begins with prefix.", |cx, a| Ok(Value::Bool(text(cx.heap, &a[0])?.starts_with(text(cx.heap, &a[1])?)))),
    native!("ends_with", 2, 2, "ends_with(string, suffix)", "Whether the string ends with suffix.", |cx, a| Ok(Value::Bool(text(cx.heap, &a[0])?.ends_with(text(cx.heap, &a[1])?)))),
    native!("chars", 1, 1, "chars(string)", "A list of the string's characters.", chars),
    native!("ord", 1, 1, "ord(char)", "The Unicode code point of a one-character string.", ord),
    native!("chr", 1, 1, "chr(code)", "The one-character string for a Unicode code point.", chr),
    // ---- maps
    native!("keys", 1, 1, "keys(map)", "A list of the map's keys, in insertion order.", keys),
    native!("values", 1, 1, "values(map)", "A list of the map's values, in insertion order.", values),
    native!("items", 1, 1, "items(map)", "A list of [key, value] pairs, in insertion order.", items),
    native!("has", 2, 2, "has(map, key)", "Whether the map contains the key.", has),
];

/// Library functions written in Rambo. Each needs a matching entry in docs/stdlib.md.
pub const PRELUDE: &str = r#"
let PI = 3.141592653589793;

fn map(xs, f) {
    let out = [];
    for x in xs { push(out, f(x)); }
    return out;
}

fn filter(xs, keep) {
    let out = [];
    for x in xs {
        if keep(x) { push(out, x); }
    }
    return out;
}

fn reduce(xs, f, initial) {
    let acc = initial;
    for x in xs { acc = f(acc, x); }
    return acc;
}

fn each(xs, f) {
    for x in xs { f(x); }
}

fn find(xs, matches) {
    for x in xs {
        if matches(x) { return x; }
    }
    return nil;
}

fn any(xs, test) {
    for x in xs {
        if test(x) { return true; }
    }
    return false;
}

fn all(xs, test) {
    for x in xs {
        if !test(x) { return false; }
    }
    return true;
}

fn sum(xs) {
    let total = 0;
    for x in xs { total += x; }
    return total;
}

fn sort_by(xs, key) {
    let pairs = [];
    for x in xs { push(pairs, [key(x), x]); }
    fn merge_sort(items) {
        if len(items) <= 1 { return items; }
        let mid = floor(len(items) / 2);
        let left = merge_sort(slice(items, 0, mid));
        let right = merge_sort(slice(items, mid));
        let out = [];
        let i = 0;
        let j = 0;
        while i < len(left) && j < len(right) {
            if right[j][0] < left[i][0] {
                push(out, right[j]);
                j += 1;
            } else {
                push(out, left[i]);
                i += 1;
            }
        }
        while i < len(left) { push(out, left[i]); i += 1; }
        while j < len(right) { push(out, right[j]); j += 1; }
        return out;
    }
    let out = [];
    for p in merge_sort(pairs) { push(out, p[1]); }
    return out;
}
"#;

/// Names defined by the prelude (kept in sync by a test).
pub const PRELUDE_NAMES: &[&str] = &["PI", "map", "filter", "reduce", "each", "find", "any", "all", "sum", "sort_by"];

// ------------------------------------------------------------------ helpers

fn num(v: &Value) -> Result<f64, String> {
    match v { Value::Num(n) => Ok(*n), _ => Err("expected a number".into()) }
}

fn text<'h>(heap: &'h Heap, v: &Value) -> Result<&'h str, String> {
    heap.as_str(v).ok_or_else(|| format!("expected a string, got {}", heap.type_name(v)))
}

fn whole(v: &Value, what: &str) -> Result<i64, String> {
    match v {
        Value::Num(n) if n.fract() == 0.0 && n.abs() < 9e15 => Ok(*n as i64),
        Value::Num(n) => Err(format!("{} must be a whole number, got {}", what, n)),
        _ => Err(format!("{} must be a number", what)),
    }
}

fn type_err(heap: &Heap, v: &Value, expected: &str) -> String {
    format!("expected {}, got {}", expected, heap.type_name(v))
}

fn list<'h>(heap: &'h Heap, v: &Value) -> Result<&'h Vec<Value>, String> {
    if let Value::Obj(g) = v { if let Obj::List(items) = heap.get(*g) { return Ok(items); } }
    Err(type_err(heap, v, "a list"))
}

fn list_mut<'h>(heap: &'h mut Heap, v: &Value) -> Result<&'h mut Vec<Value>, String> {
    let is_list = matches!(v, Value::Obj(g) if matches!(heap.get(*g), Obj::List(_)));
    if !is_list { return Err(type_err(heap, v, "a list")); }
    let Value::Obj(g) = v else { unreachable!() };
    match heap.get_mut(*g) { Obj::List(items) => Ok(items), _ => unreachable!() }
}

fn map_ref<'h>(heap: &'h Heap, v: &Value) -> Result<&'h RMap, String> {
    if let Value::Obj(g) = v { if let Obj::Map(m) = heap.get(*g) { return Ok(m); } }
    Err(type_err(heap, v, "a map"))
}

/// Clamp a possibly negative index into 0..=len (for slicing).
fn clamp_index(i: i64, len: usize) -> usize {
    let i = if i < 0 { len as i64 + i } else { i };
    i.clamp(0, len as i64) as usize
}

// ------------------------------------------------------------------ core

fn print(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    for (i, v) in a.iter().enumerate() {
        if i > 0 { cx.out.push(' '); }
        let s = cx.heap.display(v);
        cx.out.push_str(&s);
    }
    cx.out.push('\n');
    Ok(Value::Nil)
}

fn type_of(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let t = cx.heap.type_name(&a[0]);
    Ok(cx.heap.new_str(t))
}

fn to_str(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let s = cx.heap.display(&a[0]);
    Ok(cx.heap.new_str(&s))
}

fn to_num(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    if let Some(s) = cx.heap.as_str(&a[0]) {
        return Ok(s.trim().parse::<f64>().map(Value::Num).unwrap_or(Value::Nil));
    }
    match &a[0] {
        Value::Num(n) => Ok(Value::Num(*n)),
        Value::Bool(b) => Ok(Value::Num(if *b { 1.0 } else { 0.0 })),
        v => Err(format!("can't convert {} to a number", cx.heap.type_name(v))),
    }
}

fn len(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let n = match &a[0] {
        Value::Obj(g) => match cx.heap.get(*g) {
            Obj::Str(s) => s.chars().count(),
            Obj::List(l) => l.len(),
            Obj::Map(m) => m.len(),
            _ => return Err(type_err(cx.heap, &a[0], "a list, map or string")),
        },
        v => return Err(type_err(cx.heap, v, "a list, map or string")),
    };
    Ok(Value::Num(n as f64))
}

fn assert(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    if a[0].truthy() { return Ok(Value::Nil); }
    Err(match a.get(1) {
        Some(msg) => format!("assertion failed: {}", cx.heap.display(msg)),
        None => "assertion failed".into(),
    })
}

fn error(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    Err(cx.heap.display(&a[0]))
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")] // tells the linker this comes from JavaScript
extern "C" {
    /// Milliseconds from the host page (performance.now()).
    fn now() -> f64;
}

fn clock(_: &mut Ctx, _: &[Value]) -> Result<Value, String> {
    #[cfg(target_arch = "wasm32")]
    {
        use std::sync::OnceLock;
        static START: OnceLock<f64> = OnceLock::new();
        let t = unsafe { now() };
        let start = *START.get_or_init(|| t);
        Ok(Value::Num((t - start) / 1000.0))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::OnceLock;
        use std::time::Instant;
        static START: OnceLock<Instant> = OnceLock::new();
        Ok(Value::Num(START.get_or_init(Instant::now).elapsed().as_secs_f64()))
    }
}

fn gc(cx: &mut Ctx, _: &[Value]) -> Result<Value, String> {
    cx.heap.force_gc = true;
    Ok(Value::Nil)
}

fn memory(cx: &mut Ctx, _: &[Value]) -> Result<Value, String> {
    let mut m = RMap::default();
    let (live, collections, freed) = (cx.heap.live(), cx.heap.collections, cx.heap.total_freed);
    m.insert(Key::Str(cx.heap.intern("live")), Value::Num(live as f64));
    m.insert(Key::Str(cx.heap.intern("collections")), Value::Num(collections as f64));
    m.insert(Key::Str(cx.heap.intern("freed")), Value::Num(freed as f64));
    Ok(cx.heap.new_map(m))
}

// ------------------------------------------------------------------ random

/// xorshift64*: small, fast, and good enough for games and simulations.
fn next_random(state: &mut u64) -> f64 {
    let mut x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    (x.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
}

fn random(cx: &mut Ctx, _: &[Value]) -> Result<Value, String> {
    Ok(Value::Num(next_random(cx.rng)))
}

fn random_int(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let (lo, hi) = (whole(&a[0], "low")?, whole(&a[1], "high")?);
    if hi < lo { return Err("high must be at least low".into()); }
    let span = (hi - lo + 1) as f64;
    Ok(Value::Num((lo as f64 + (next_random(cx.rng) * span).floor()).min(hi as f64)))
}

fn seed(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let n = num(&a[0])?;
    *cx.rng = (n.to_bits() ^ 0x9E3779B97F4A7C15).max(1);
    Ok(Value::Nil)
}

// ------------------------------------------------------------------ lists

fn push(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    list_mut(cx.heap, &a[0])?.push(a[1]);
    Ok(Value::Nil)
}

fn pop(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    list_mut(cx.heap, &a[0])?.pop().ok_or_else(|| "can't pop from an empty list".to_string())
}

fn insert(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let i = whole(&a[1], "index")?;
    let items = list_mut(cx.heap, &a[0])?;
    let len = items.len() as i64;
    let at = if i < 0 { len + i } else { i };
    if at < 0 || at > len { return Err(format!("index {} out of range for insert (length {})", i, len)); }
    items.insert(at as usize, a[2]);
    Ok(Value::Nil)
}

fn remove(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let key = Key::from_value(cx.heap, &a[1]);
    if let Value::Obj(g) = &a[0] {
        match cx.heap.get_mut(*g) {
            Obj::List(items) => {
                let i = whole(&a[1], "index")?;
                let len = items.len() as i64;
                let at = if i < 0 { len + i } else { i };
                if at < 0 || at >= len { return Err(format!("index {} out of range (length {})", i, len)); }
                return Ok(items.remove(at as usize));
            }
            Obj::Map(m) => return Ok(m.remove(&key?).unwrap_or(Value::Nil)),
            _ => {}
        }
    }
    Err(type_err(cx.heap, &a[0], "a list or map"))
}

fn slice(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let start = whole(&a[1], "start")?;
    if let Some(s) = cx.heap.as_str(&a[0]) {
        let chars: Vec<char> = s.chars().collect();
        let end = match a.get(2) { Some(v) => whole(v, "end")?, None => chars.len() as i64 };
        let (s0, e0) = (clamp_index(start, chars.len()), clamp_index(end, chars.len()));
        let out: String = chars[s0..e0.max(s0)].iter().collect();
        return Ok(cx.heap.new_str(&out));
    }
    match &a[0] {
        v => {
            let items = list(cx.heap, v)?;
            let end = match a.get(2) { Some(v) => whole(v, "end")?, None => items.len() as i64 };
            let (s0, e0) = (clamp_index(start, items.len()), clamp_index(end, items.len()));
            let copy = items[s0..e0.max(s0)].to_vec();
            Ok(cx.heap.new_list(copy))
        }
    }
}

fn contains(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    match &a[0] {
        Value::Obj(g) => match cx.heap.get(*g) {
            Obj::Str(s) => Ok(Value::Bool(s.contains(text(cx.heap, &a[1])?))),
            Obj::List(items) => Ok(Value::Bool(items.iter().any(|x| cx.heap.equals(x, &a[1])))),
            Obj::Map(m) => Ok(Value::Bool(Key::from_value(cx.heap, &a[1]).map(|k| m.get(&k).is_some()).unwrap_or(false))),
            _ => Err(type_err(cx.heap, &a[0], "a list, map or string")),
        },
        v => Err(type_err(cx.heap, v, "a list, map or string")),
    }
}

fn index_of(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    if let Some(s) = cx.heap.as_str(&a[0]) {
        let needle = text(cx.heap, &a[1])?;
        return Ok(match s.find(needle) {
            Some(byte) => Value::Num(s[..byte].chars().count() as f64),
            None => Value::Nil,
        });
    }
    match &a[0] {
        v => {
            let items = list(cx.heap, v)?;
            Ok(match items.iter().position(|x| cx.heap.equals(x, &a[1])) {
                Some(i) => Value::Num(i as f64),
                None => Value::Nil,
            })
        }
    }
}

fn reverse(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    if let Some(s) = cx.heap.as_str(&a[0]) {
        let out: String = s.chars().rev().collect();
        return Ok(cx.heap.new_str(&out));
    }
    match &a[0] {
        v => {
            let mut copy = list(cx.heap, v)?.clone();
            copy.reverse();
            Ok(cx.heap.new_list(copy))
        }
    }
}

fn sort(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let mut copy = list(cx.heap, &a[0])?.clone();
    let heap = &*cx.heap;
    if copy.iter().all(|v| matches!(v, Value::Num(_))) {
        copy.sort_by(|x, y| match (x, y) {
            (Value::Num(p), Value::Num(q)) => p.total_cmp(q),
            _ => unreachable!(),
        });
    } else if copy.iter().all(|v| heap.as_str(v).is_some()) {
        copy.sort_by(|x, y| heap.as_str(x).unwrap().cmp(heap.as_str(y).unwrap()));
    } else {
        return Err("can only sort a list of all numbers or all strings (use sort_by for anything else)".into());
    }
    Ok(cx.heap.new_list(copy))
}

fn join(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let sep = text(cx.heap, &a[1])?;
    let items = list(cx.heap, &a[0])?;
    let parts: Vec<String> = items.iter().map(|v| cx.heap.display(v)).collect();
    let joined = parts.join(sep);
    Ok(cx.heap.new_str(&joined))
}

fn range(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let (start, end, step) = match a.len() {
        1 => (0.0, num(&a[0])?, 1.0),
        2 => (num(&a[0])?, num(&a[1])?, 1.0),
        _ => (num(&a[0])?, num(&a[1])?, num(&a[2])?),
    };
    if step == 0.0 { return Err("step can't be zero".into()); }
    let count = ((end - start) / step).ceil();
    if count > 10_000_000.0 { return Err("range too large (over 10 million items)".into()); }
    let items: Vec<Value> = (0..count.max(0.0) as usize).map(|i| Value::Num(start + i as f64 * step)).collect();
    Ok(cx.heap.new_list(items))
}

// ------------------------------------------------------------------ strings

fn split(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let (s, sep) = (text(cx.heap, &a[0])?, text(cx.heap, &a[1])?);
    let pieces: Vec<String> = if sep.is_empty() {
        s.chars().map(|c| c.to_string()).collect()
    } else {
        s.split(sep).map(str::to_string).collect()
    };
    let parts = pieces.iter().map(|p| cx.heap.new_str(p)).collect();
    Ok(cx.heap.new_list(parts))
}

fn replace(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let (s, from, to) = (text(cx.heap, &a[0])?, text(cx.heap, &a[1])?, text(cx.heap, &a[2])?);
    if from.is_empty() { return Err("the text to replace can't be empty".into()); }
    let out = s.replace(from, to);
    Ok(cx.heap.new_str(&out))
}

fn chars(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let pieces: Vec<String> = text(cx.heap, &a[0])?.chars().map(|c| c.to_string()).collect();
    let items = pieces.iter().map(|p| cx.heap.new_str(p)).collect();
    Ok(cx.heap.new_list(items))
}

fn ord(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let s = text(cx.heap, &a[0])?;
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(c), None) => Ok(Value::Num(c as u32 as f64)),
        _ => Err("expected a single character".into()),
    }
}

fn chr(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let n = whole(&a[0], "code")?;
    match u32::try_from(n).ok().and_then(char::from_u32) {
        Some(c) => Ok(cx.heap.new_str(&c.to_string())),
        None => Err(format!("{} is not a valid character code", n)),
    }
}

// ------------------------------------------------------------------ maps

fn keys(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let items = map_ref(cx.heap, &a[0])?.entries.iter().map(|(k, _)| k.to_value()).collect();
    Ok(cx.heap.new_list(items))
}

fn values(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let items = map_ref(cx.heap, &a[0])?.entries.iter().map(|(_, v)| *v).collect();
    Ok(cx.heap.new_list(items))
}

fn items(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let pairs: Vec<(Value, Value)> = map_ref(cx.heap, &a[0])?.entries.iter().map(|(k, v)| (k.to_value(), *v)).collect();
    let lists: Vec<Value> = pairs.into_iter().map(|(k, v)| cx.heap.new_list(vec![k, v])).collect();
    Ok(cx.heap.new_list(lists))
}

fn has(cx: &mut Ctx, a: &[Value]) -> Result<Value, String> {
    let m = map_ref(cx.heap, &a[0])?;
    Ok(Value::Bool(Key::from_value(cx.heap, &a[1]).map(|k| m.get(&k).is_some()).unwrap_or(false)))
}
