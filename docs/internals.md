# How it works

Rambo is about 3,800 lines of Rust with no dependencies. This page walks through the pipeline a program goes through, the design decisions behind each stage, and how to read the code.

```text
source ──▶ lexer ──▶ tokens ──▶ parser ──▶ AST ──▶ compiler ──▶ bytecode ──▶ VM
         lexer.rs            parser.rs         compiler.rs              vm.rs
```

| File | Role |
|---|---|
| `lexer.rs` | Turns text into tokens, each tagged with line and column |
| `parser.rs`, `ast.rs` | Recursive-descent parser producing a syntax tree |
| `compiler.rs` | One pass over the tree, emitting bytecode |
| `bytecode.rs` | The instruction set and the disassembler |
| `value.rs` | Values, the heap, the garbage collector, maps |
| `vm.rs` | The virtual machine |
| `natives.rs` | The standard library, in Rust and in Rambo |
| `debug.rs` | The step-through debugger |
| `wasm.rs` | The WebAssembly interface for the browser |

## Lexing

The lexer reads characters and produces tokens like `Let`, `Ident("x")` and `Number(4.0)`. Each token records its line and column, which is how every later error can point at the exact spot.

String interpolation is resolved here. `"hi {name}!"` becomes one `Interp` token holding the literal parts and the *source text* of each `{...}` along with where it starts. The parser then lexes and parses that text as an ordinary expression. Nested braces and even nested strings (`"{f("x")}"`) work because the lexer tracks brace depth and skips over string literals while looking for the closing `}`.

## Parsing

The parser is recursive descent: one function per grammar rule, with expressions handled by one function per precedence level (see [Grammar](grammar.md)). A few conveniences are *desugared*, that is, rewritten into simpler forms during parsing:

* `a.b` becomes `a["b"]`
* `else if` becomes `else { if ... }`
* `"x = {x}"` becomes a list of parts that the compiler joins

A missing semicolon is the most common syntax error, so it gets special treatment: if the next token is on a new line, the error points just past the end of the previous line, where the `;` belongs.

## Compiling

The compiler walks the syntax tree once and emits instructions for a stack machine. `1 + 2 * 3` becomes:

```text
CONST 1
CONST 2
CONST 3
MUL
ADD
```

The interesting part is **variables**. Each is resolved at compile time into one of three kinds, so the VM never looks a variable up by name:

* **Locals** live in slots on the VM's value stack. A `let` statement doesn't even need an instruction: the value it computes is already on top of the stack, and that slot simply *becomes* the variable. `GET_LOCAL 3` reads slot 3 of the current function's window.
* **Upvalues** are local variables of an *enclosing* function that a closure uses. The compiler records, for each closure, which outer variables it needs.
* **Globals** are top-level declarations. Each name gets a numbered slot when first seen, so `GET_GLOBAL 56` is an array access. Because slots are assigned at compile time but filled at runtime, top-level functions can call each other regardless of declaration order.

You can see all three with `rambo dis`:

```text
== make_counter (params: 0, upvalues: 0) ==
0000    2  CONST           0       ; 0
0001    3  CLOSURE         0       ; <anonymous>, captures count
0002    |  RETURN

== <anonymous> (params: 0, upvalues: 1) ==
0000    4  GET_UPVALUE     0       ; count
0001    |  CONST           0       ; 1
0002    |  ADD
0003    |  SET_UPVALUE     0       ; count
```

## Closures and upvalues

Captured variables are the trickiest part of any language implementation, because a closure can outlive the function that created its variables. Rambo uses the design from Lua (also described in *Crafting Interpreters*):

1. While the outer function is still running, a captured variable stays in its stack slot. The closure holds an **open upvalue**, a pointer to that slot, so both see the same variable.
2. When the variable goes out of scope, the compiler emits `CLOSE_UPVALUE` instead of `POP`. The VM copies the value into the upvalue object itself, which is now **closed**, and the closure keeps working on its own copy.

The VM keeps open upvalues in a list sorted by stack slot, so two closures capturing the same variable share one upvalue and see each other's changes.

**Fresh loop variables.** A `for` loop keeps its real counter in a hidden local (shown as `@i` in the disassembly) and copies it into a new local `i` at the start of each iteration. The body's scope ends every iteration, which closes any upvalue capturing `i`, so each closure made in the loop keeps its own value. Rambo's predecessor, Ember, got this wrong, and it's a classic bug in real languages too (C# changed its `foreach` semantics in version 5 for exactly this reason).

## The virtual machine

The VM (`vm.rs`) is a loop that fetches an instruction, executes it, and repeats. There's one value stack shared by every call; a call frame is just a starting position (`base`) into that stack plus an instruction pointer. Calling a function pushes a frame whose window starts at the function value itself, so arguments are already in place as its first locals, with no copying.

The VM is **resumable**: `run(budget)` executes at most `budget` instructions and returns `Paused`, and calling it again picks up exactly where it stopped. The debugger drives the VM one instruction at a time this way, and the browser uses the budget as a guard against infinite loops.

### Instruction reference

"Stack" shows values consumed ▸ produced, top of stack on the right.

| Instruction | Stack | Description |
|---|---|---|
| `CONST k` | ▸ v | Push constant `k` |
| `NIL`, `TRUE`, `FALSE` | ▸ v | Push a literal |
| `POP` | v ▸ | Discard the top value |
| `DUP2` | a b ▸ a b a b | Duplicate the top two (for `xs[i] += 1`) |
| `GET_LOCAL s` | ▸ v | Push local slot `s` |
| `SET_LOCAL s` | v ▸ | Store into local slot `s` |
| `GET_UPVALUE i` | ▸ v | Push captured variable `i` |
| `SET_UPVALUE i` | v ▸ | Store into captured variable `i` |
| `CLOSE_UPVALUE` | v ▸ | Move a captured local off the stack into its upvalue |
| `GET_GLOBAL g` | ▸ v | Push global `g`; error if undefined |
| `SET_GLOBAL g` | v ▸ | Store into global `g`; error if undefined |
| `DEFINE_GLOBAL g` | v ▸ | Create or overwrite global `g` |
| `ADD` `SUB` `MUL` `DIV` `MOD` | a b ▸ r | Arithmetic; `ADD` also joins strings and lists |
| `EQ` `NE` `LT` `LE` `GT` `GE` | a b ▸ bool | Comparison |
| `NEG`, `NOT` | a ▸ r | Negate, logical not |
| `JUMP t` | | Continue at `t` |
| `JUMP_IF_FALSE t` | c ▸ | Jump if `c` is falsy |
| `JUMP_IF_NOT_LT t` (also `LE`, `GT`, `GE`) | a b ▸ | Fused compare-and-branch: jump unless `a < b` |
| `JUMP_FALSE_KEEP t`, `JUMP_TRUE_KEEP t` | c ▸ c | Conditional jump that keeps `c` (for `&&` and `\|\|`) |
| `CALL n` | f a₁…aₙ ▸ r | Call `f` with `n` arguments |
| `RETURN` | r ▸ | Return `r` to the caller |
| `CLOSURE p` | ▸ f | Create a closure from function `p`, capturing its upvalues |
| `MAKE_LIST n` | v₁…vₙ ▸ list | Build a list |
| `MAKE_MAP n` | k₁ v₁…kₙ vₙ ▸ map | Build a map |
| `CONCAT n` | v₁…vₙ ▸ str | Join printed forms (string interpolation) |
| `INDEX` | obj key ▸ v | `obj[key]` |
| `SET_INDEX` | obj key v ▸ | `obj[key] = v` |
| `ITER_PREP` | c ▸ seq | Prepare a `for` loop's collection (a map becomes its keys) |
| `LEN` | seq ▸ n | Length, for `for` loops |

## Values and the heap

A `Value` is a 16-byte, freely copyable enum: `Nil`, `Bool`, `Num(f64)`, a pointer to a built-in function, or `Obj(Gc)`. A `Gc` is just a 32-bit index into the heap's object table. Lists, maps, closures, upvalues **and strings** are heap objects.

**String interning.** Every distinct string exists exactly once in the heap; creating a string first checks a hash table of existing ones. This has three payoffs:

* `Value` needs no reference counting, so copying one is a plain memory copy and the VM never runs cleanup code when values are overwritten.
* Comparing two strings for equality is comparing two integers.
* Maps key strings by their handle, so `dog.name` hashes 4 bytes instead of the whole string.

Maps keep an insertion-ordered list of entries alongside a hash index, which is why printing and iterating a map is predictable.

## The garbage collector

Rambo uses a **mark-and-sweep** collector (`Heap::collect` in `value.rs`):

1. **Mark.** Start from the *roots*: every value on the VM stack, every global, each running function's closure, open upvalues, and string constants in compiled code. Follow every reference from there, marking each object reached.
2. **Sweep.** Every object not marked is unreachable. Free it, and put its slot on a free list for reuse.

The intern table holds its strings *weakly*: after marking, entries for unmarked strings are removed before sweeping, so unused strings don't live forever.

Unlike reference counting (which Rambo's predecessor used), tracing finds everything reachable, so **cycles are collected**: a map that contains itself is freed as soon as nothing else points at it.

A collection starts once the number of live objects reaches a threshold, which then resets to twice the number of survivors. This keeps collection cost proportional to allocation.

**Safe points.** The collector only runs at *safe points*: backward jumps (the end of a loop iteration) and function calls. Every live value is guaranteed to be on the stack or in a root there, never held in a temporary inside a half-finished instruction. And because straight-line code can only allocate a bounded amount before reaching a loop or call, checking at these points is enough. The check is also out of the per-instruction hot path. See [Performance](performance.md).

## The debugger

`debug.rs` wraps a paused VM. Each command runs the VM one instruction at a time until its stopping rule is met:

| Command | Stops when |
|---|---|
| Step | the source line changes, or a call starts or returns |
| Over | the line changes in the same function, or the function returns |
| Out | the current function returns |
| Continue | execution arrives at a breakpoint line |

Library functions written in Rambo (like `map`) are run through without stopping, since their code isn't in your editor. To show local variables by name, the compiler records for every local its name, slot, and the range of instructions during which it exists, a simplified version of what real compilers put in debug info.

## WebAssembly

The browser runs the same Rust code compiled to WebAssembly, without `wasm-bindgen`. The interface is a handful of exported functions over linear memory:

1. JavaScript calls `alloc(n)` and copies the UTF-8 source into that buffer.
2. It calls `run`, `disassemble`, `dbg_start`, `dbg_step` or `dbg_breakpoints`.
3. Rust writes a JSON result into a buffer, and JavaScript reads it via `result_ptr()` and `result_len()`.

The module imports one function, `env.now`, which `clock()` uses for timing.

## Testing

`cargo test` runs:

* **Language tests**: programs checked against their exact output, including every error message.
* **Standard library tests**: every built-in, plus a test that fails if any built-in is missing from [the reference](stdlib.md).
* **GC tests**: cycles are freed, and reachable data survives thousands of collections intact.
* **Debugger tests**: each stepping mode stops where it should.
* **Fuzz tests**: 6,000 random and randomly mutated programs. Any input must produce a result or an error, never a crash.
