# Changelog

## 0.2.0: Rambo

The language was renamed from Ember to Rambo, and the implementation rebuilt.

### Language
- Maps: `{name: "Rambo"}`, with dot access (`dog.name`) and computed keys (`{[k]: v}`)
- String interpolation: `"hi {name}"`
- Compound assignment: `+=`, `-=`, `*=`, `/=`, `%=`
- Block comments, `else if`, negative indexes, trailing commas
- `for x in map` loops over keys
- About 55 standard library functions, including `map`, `filter`, `reduce` and `sort_by` written in Rambo itself
- File extension is now `.rmb`

### Fixed
- Closures created in a loop captured one shared loop variable, so every closure saw the final value. Each iteration now gets a fresh variable.
- Reference cycles (a list containing itself, a closure stored in a variable it captures) leaked memory. They're now collected.

### Implementation
- Compile-time resolution of locals, upvalues and globals (previously every function call allocated a heap frame and globals were looked up by name)
- Mark-and-sweep garbage collector, replacing reference counting
- Interned strings, making `Value` a 16-byte `Copy` type
- Fused compare-and-branch superinstructions
- Rust-style error messages with column carets and stack traces
- Resumable VM and a step-through debugger in the playground
- About 4× faster overall (see `docs/performance.md`)
- Documentation site, fuzz tests, GC and debugger tests

## 0.1.0: Ember

First version: a tree of Rust modules (lexer, parser, compiler, VM) with numbers, strings, lists, functions and closures, compiled to WebAssembly for a browser playground.
