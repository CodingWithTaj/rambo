# Functions

## Declaring and calling

```rambo
fn greet(name, trick) {
    return "{name}, {trick}!";
}
print(greet("Rambo", "sit"));
```

A function without a `return` (or with a bare `return;`) returns `nil`. Calling a function with the wrong number of arguments is an error; Rambo doesn't silently fill in `nil`.

Top-level functions can call each other no matter which is declared first:

```rambo
fn is_even(n) { if n == 0 { return true; } return is_odd(n - 1); }
fn is_odd(n) { if n == 0 { return false; } return is_even(n - 1); }
print(is_even(10));
```

## Functions are values

Functions can be stored in variables, passed as arguments and returned from other functions. `fn(...) { ... }` without a name creates an anonymous function.

```rambo
let double = fn(x) { return x * 2; };

fn apply_twice(f, x) {
    return f(f(x));
}
print(apply_twice(double, 5));
```

## Closures

A function can use variables from the scope it was created in, even after that scope has finished running. It's said to **capture** them. Captured variables are shared, not copied, so changes are visible to everyone who captured them.

```rambo
fn make_counter() {
    let count = 0;
    return fn() {
        count += 1;
        return count;
    };
}

let a = make_counter();
let b = make_counter();
a(); a();
print(a(), b());   // 3 1: each counter has its own count
```

Loop variables are fresh in each iteration, so closures made in a loop remember the value from their own iteration:

```rambo
let fs = [];
for i in 0..3 {
    push(fs, fn() { return i; });
}
print(map(fs, fn(f) { return f(); }));   // [0, 1, 2]
```

(This is a classic bug in many languages. It was one in Rambo's predecessor, Ember, too; see `CHANGELOG.md`.)

## Recursion

Functions can call themselves. Local functions can too.

```rambo
fn factorial(n) {
    if n <= 1 { return 1; }
    return n * factorial(n - 1);
}
print(factorial(10));
```

Calls can nest up to 1000 deep. Beyond that Rambo stops with a "stack overflow" error instead of crashing, which usually means a recursive function is missing its base case.

## Higher-order helpers

The standard library includes `map`, `filter`, `reduce`, `each`, `find`, `any`, `all`, `sum` and `sort_by`, all taking functions as arguments:

```rambo
let nums = range(1, 11);
let evens = filter(nums, fn(n) { return n % 2 == 0; });
print(evens, sum(evens));
```

These are written in Rambo itself. You can read them in the prelude in `src/natives.rs`.
