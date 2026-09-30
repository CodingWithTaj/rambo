# Getting started

Rambo is a small, fast scripting language. Programs compile to bytecode and run on a virtual machine written in Rust, which also compiles to WebAssembly, so the same interpreter runs on your computer and in your browser.

The quickest way to try it is the [playground](../playground/index.html): no install needed. Every code block in these docs also has a **Run** button.

```rambo
let name = "world";
print("Hello, {name}!");
```

## Installing from source

You need [Rust](https://rustup.rs). Then:

```bash
git clone https://github.com/CodingWithTaj/rambo.git
cd rambo
cargo build --release
```

The `rambo` binary is now in `target/release/`. Check it works:

```bash
./target/release/rambo --version
```

> **Windows:** the binary is `target\release\rambo.exe`, and you can use backslashes in paths.

## Running programs

Rambo files end in `.rmb`. Save this as `hello.rmb`:

```rambo
let tricks = ["sit", "shake", "roll over"];
for trick in tricks {
    print("Rambo can {trick}!");
}
```

and run it:

```bash
rambo hello.rmb
```

The `examples/` folder has more programs to try.

## The command line

| Command | What it does |
|---|---|
| `rambo` | Start the interactive REPL |
| `rambo file.rmb` | Run a program |
| `rambo dis file.rmb` | Show the bytecode a program compiles to |
| `rambo builtins` | List every built-in function |
| `rambo --version` | Print the version |

Exit codes follow the BSD `sysexits` convention: `65` for syntax and compile errors, `70` for runtime errors, `66` if the file can't be read.

## The REPL

Run `rambo` with no arguments to get an interactive prompt. Type an expression to see its value; variables you define stay around for later lines. Multi-line functions work too: the REPL keeps reading until your brackets balance.

```text
rambo> let dog = {name: "Rambo", age: 4}
rambo> dog.age * 7
28
rambo> fn greet(d) {
  ...>     return "hi {d.name}";
  ...> }
rambo> greet(dog)
hi Rambo
```

## Where to next

Read the language guide in order: [Basics](basics.md), [Control flow](control-flow.md), [Functions](functions.md), [Collections](collections.md) and [Errors & debugging](errors.md). The [Standard library](stdlib.md) lists every built-in function, and [How it works](internals.md) explains the implementation.
