# Performance

Rambo 0.2 is about 4× faster than its predecessor and within 1.4–2× of CPython. This page shows the numbers and, more usefully, how they were found and fixed.

## Benchmarks

Best of three runs, in seconds (lower is better). Measured on one core of an Intel Xeon at 2.1 GHz with Python 3.12.3 and Lua 5.4.6. The programs are in `benches/`; run `python3 benches/run.py` to reproduce.

| Benchmark | What it tests | Ember 0.1 | **Rambo 0.2** | Python 3.12 | Lua 5.4 |
|---|---|---|---|---|---|
| fib | `fib(30)`, 1.6M recursive calls | 0.48 | **0.11** | 0.08 | 0.05 |
| loop | 10M iterations of arithmetic | 1.70 | **0.48** | 0.32 | 0.10 |
| closures | 2M calls to a closure | 0.49 | **0.11** | 0.08 | 0.04 |
| sieve | primes below 2M, list-heavy | 1.75 | **0.40** | 0.20 | 0.10 |

Lua is fastest because it uses a *register-based* VM, where one instruction like `ADD r1 r2 r3` does the work of Rambo's four stack instructions. Moving Rambo to that design would be the next big step.

## How the speed-ups were found

Wall-clock times in a shared environment vary by ±10%, too noisy to judge small changes. Instead, each change was measured with **callgrind** (part of Valgrind), which counts the exact number of machine instructions a program executes and attributes them to source lines. The test program was a 300,000-iteration counting loop.

```bash
valgrind --tool=callgrind ./target/release/rambo loop.rmb
callgrind_annotate --auto=yes callgrind.out.*
```

| Change | Machine instructions | Per bytecode instruction |
|---|---|---|
| Starting point | 168.7M | ~62 |
| Per-instruction checks moved out of the hot loop | 140.7M | ~52 |
| Fused compare-and-branch instructions | 123.1M | ~51 |
| Interned strings: `Value` becomes `Copy` | 90.0M | ~38 |
| List-index fast paths | 88.8M | ~38 |

### 1. Get bookkeeping out of the hot loop

The first profile showed ~25% of all work was bookkeeping *around* instructions rather than in them: counting steps, checking the step limit, checking whether the GC should run, and checking whether output should be flushed, all on **every** instruction. Rust must reload these fields from memory each time, because anything in the loop might have changed them.

The fix: one local countdown covers both the caller's budget and the step limit (steps are totalled once at exit), output is flushed only after built-in calls (the only place output is produced), and GC checks moved to [safe points](internals.md#the-garbage-collector).

### 2. Superinstructions

Nearly every loop condition compiles to a comparison followed by a conditional jump: `LT` pushes a boolean, and `JUMP_IF_FALSE` immediately pops it. The compiler now fuses these into one `JUMP_IF_NOT_LT`, skipping the boolean entirely.

The subtle part is *when fusion is safe*. In `if a || b < c`, the `||` jumps straight to the conditional jump, landing *between* the two instructions. Fusing there would skip the jump entirely and break the program. So the compiler records every jump target and only fuses when nothing lands between the pair.

### 3. The hidden cost of reference counting

This was the big one. Strings used to be reference-counted (`Rc<str>`), which meant `Value` had a destructor. So every time *any* value was copied, popped or overwritten, even a plain number, the generated code had to check "is this a string whose count needs updating?" Number-crunching code paid a tax for strings it never used.

Moving strings into the garbage-collected heap and interning them removed the destructor completely. `Value` became a plain 16-byte `Copy` type (down from 24 bytes), copying one became a memory copy, and the machine instruction count fell by 27% in one change. As side benefits, string equality became an integer comparison and map lookups by string key no longer hash the string's contents.

The changes were safe to make because of the test suite: this refactor touched the core of every file, and the existing tests, fuzz tests and new GC-specific tests caught every mistake along the way.

## What's next

Profiles now show the remaining ~38 machine instructions per bytecode instruction spread evenly across dispatch, stack access and bounds checks, with no single hotspot left. Further gains would need architectural changes:

* a register-based instruction set, like Lua's
* computing each function's maximum stack depth at compile time, so pushes need no capacity checks
* NaN-boxing, packing every value into 8 bytes
