# Rambo

[![CI](https://github.com/CodingWithTaj/rambo/actions/workflows/deploy.yml/badge.svg)](https://github.com/CodingWithTaj/rambo/actions/workflows/deploy.yml)

A small, fast scripting language with closures, maps, a garbage collector and a step-through debugger. Written in Rust with no dependencies, and compiled to WebAssembly so it runs in the browser too.

**[Try it in the playground](https://CodingWithTaj.github.io/rambo/playground/)** · **[Read the docs](https://CodingWithTaj.github.io/rambo/docs/getting-started.html)**

```rust
let dog = {name: "Rambo", tricks: ["sit", "shake"]};
push(dog.tricks, "roll over");

fn show_off(d) {
    for trick in d.tricks {
        print("{d.name} can {trick}!");
    }
    return len(d.tricks);
}

print("That's {show_off(dog)} tricks. Good boy.");
```

Named after my dog, Rambo. Unlike its namesake, it cleans up after itself.

## Features

- **The language:** numbers, strings, lists, maps, first-class functions and closures, string interpolation, `for` loops over ranges and collections, compound assignment, and a standard library of about 55 functions.
- **A bytecode compiler and VM**, with variables resolved at compile time into stack slots, upvalues and numbered globals, the same design Lua uses.
- **A mark-and-sweep garbage collector** that frees cycles, with interned strings so every value is a plain 16-byte copy.
- **Errors that point** at the exact line and column, with a stack trace.
- **A browser playground** with syntax highlighting, a bytecode view and a step-through debugger (step into, over and out, breakpoints, and live views of locals, captured variables and the value stack).
- **Documentation** with runnable examples on every page, a full standard library reference and a formal grammar.

## Quick start

You need [Rust](https://rustup.rs).

```bash
cargo build --release
./target/release/rambo examples/hello.rmb          # run a program
./target/release/rambo                             # interactive REPL
./target/release/rambo dis examples/closures.rmb   # show the bytecode
cargo test                                         # run the test suite
```

On Windows, the binary is `target\release\rambo.exe`.

### Building the website

```bash
rustup target add wasm32-unknown-unknown
cargo build --lib --release --target wasm32-unknown-unknown
python web/build.py
```

Then serve `web/dist/` with any static server (for example `python -m http.server -d web/dist`) and open it in a browser. `web/dist/rambo-playground.html` is a single self-contained file that works when opened directly. Pushing to `main` builds and deploys the site to GitHub Pages automatically.

## How it works

```text
source → lexer → tokens → parser → AST → compiler → bytecode → VM
```

| File | Role |
|---|---|
| `src/lexer.rs` | Text to tokens, including string interpolation |
| `src/parser.rs` | Recursive-descent parser |
| `src/compiler.rs` | Single-pass compiler; resolves locals, upvalues and globals |
| `src/vm.rs` | Resumable stack VM |
| `src/value.rs` | Values, heap, garbage collector, string interning |
| `src/natives.rs` | Standard library (in Rust, plus a prelude written in Rambo) |
| `src/debug.rs` | Step-through debugger |
| `src/wasm.rs` | WebAssembly interface (no `wasm-bindgen`) |

The [internals guide](docs/internals.md) explains each stage, including closures, the garbage collector and the full instruction set.

## Performance

Best of three runs, in seconds:

| Benchmark | Ember 0.1 | **Rambo 0.2** | Python 3.12 | Lua 5.4 |
|---|---|---|---|---|
| fib(30) | 0.48 | **0.11** | 0.08 | 0.05 |
| 10M-iteration loop | 1.70 | **0.48** | 0.32 | 0.10 |
| 2M closure calls | 0.49 | **0.11** | 0.08 | 0.04 |
| Sieve of 2M | 1.75 | **0.40** | 0.20 | 0.10 |

The speed-ups came from profiling with callgrind: moving bookkeeping out of the instruction loop, fused compare-and-branch instructions, and interning strings so values no longer need reference counting. [The performance write-up](docs/performance.md) walks through each step with instruction counts. Run `python benches/run.py` to reproduce.

## Testing

`cargo test` runs 65 tests: language behaviour and every error message, every standard library function, garbage collector correctness under stress, each debugger stepping mode, and a fuzzer that feeds 6,000 random and mutated programs to the compiler and VM to check nothing ever crashes. A test also fails if any built-in function is missing from the documentation.

## License

MIT
