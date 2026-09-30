# Errors & debugging

## Reading error messages

Every error says what went wrong and points at the exact spot:

```text
runtime error: can't subtract string and number
  --> line 2, column 20
  |
2 | print(dog.name + 1 - 2);
  |                    ^
  = at <script> (line 2)
```

There are three kinds:

* **syntax error**: the code isn't valid Rambo, like a missing `;` or unclosed `{`. Nothing runs.
* **compile error**: valid syntax, but something that can't work, like `break` outside a loop.
* **runtime error**: something went wrong while running, like adding a number to `nil`. The message includes a **stack trace**: which function was running, and who called it.

Run this to see a trace through two functions:

```rambo
fn average(xs) {
    return sum(xs) / len(xs);
}
fn report(scores) {
    print("average: {average(scores)}");
}
report([4, 8, 15]);
report([]);
```

## Raising your own errors

`error(message)` stops the program with a runtime error. `assert(condition, message)` stops it if the condition is false; use it to check your assumptions.

```rambo
fn walk(minutes) {
    assert(minutes > 0, "a walk needs a positive length, got {minutes}");
    return "walked for {minutes} minutes";
}
print(walk(20));
print(walk(-5));
```

## Infinite loops

The command line lets programs run as long as they like. In the browser, the playground and these docs stop a program after 50 million instructions, so a runaway loop can't freeze the page.

## The debugger

The [playground](../playground/index.html) has a step-through debugger:

1. Click in the gutter next to a line number to set a **breakpoint** (click again to remove it).
2. Press **Debug**. The program pauses before its first line.
3. **Step** runs to the next line, entering any function that's called. **Over** runs to the next line in the *current* function, treating calls as one step. **Out** runs until the current function returns. **Continue** runs to the next breakpoint.

While paused, the panel shows the call stack with each function's local variables and captured variables, the VM's value stack, and global variables. The current line is highlighted in the editor, and in the Bytecode tab you can see the exact instruction about to run.
