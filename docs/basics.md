# Basics

## Comments

```rambo
// A line comment runs to the end of the line.
/* A block comment
   can span several lines. */
print("comments are ignored");
```

## Values and types

Rambo has seven types. `type(x)` tells you which one a value has.

| Type | Examples | Notes |
|---|---|---|
| `nil` | `nil` | "no value" |
| `bool` | `true`, `false` | |
| `number` | `42`, `3.14`, `1_000_000` | 64-bit floating point; `_` separators allowed |
| `string` | `"hello"` | immutable text |
| `list` | `[1, 2, 3]` | ordered, growable |
| `map` | `{name: "Rambo"}` | key–value pairs |
| `function` | `fn(x) { return x; }` | functions are values |

```rambo
print(type(4), type("four"), type([4]), type(nil));
```

## Variables

Declare variables with `let`. Every variable needs a starting value. After that, assign with `=`.

```rambo
let age = 4;
age = age + 1;
print(age);
```

Assigning to a name that was never declared is an error, which catches typos:

```rambo
let score = 10;
scroe = 11;   // error: 'scroe' is not defined
```

Compound assignment operators update a variable in place: `+=`, `-=`, `*=`, `/=` and `%=`.

```rambo
let treats = 3;
treats += 2;
treats *= 10;
print(treats);   // 50
```

Variables declared at the top level of a file are **global**. Variables declared inside a block `{ ... }` are **local** to that block, and a local can shadow an outer variable with the same name.

```rambo
let x = "outer";
{
    let x = "inner";
    print(x);   // inner
}
print(x);       // outer
```

## Numbers

All numbers are 64-bit floats, like JavaScript and Lua. Whole numbers print without a decimal point.

```rambo
print(7 / 2, 7 * 2, 2 + 3 * 4, (2 + 3) * 4);
print(0.1 + 0.2);
```

`%` is the remainder. Its result always has the same sign as the divisor, so `-7 % 3` is `2` (handy for wrapping around).

Division or remainder by zero is a runtime error rather than a silent `inf` or `nan`.

## Strings

Strings are written in double quotes and can't be changed in place; every string operation returns a new string. They're Unicode, and `len`, indexing and slicing count characters, not bytes.

```rambo
let s = "héllo";
print(len(s), s[1], s[-1], upper(s));
```

**Interpolation:** anything inside `{ }` in a string is evaluated and inserted.

```rambo
let name = "Rambo";
let age = 4;
print("{name} is {age * 7} in dog years");
```

Use `\{` and `\}` for literal braces. Other escapes are `\n` (newline), `\t` (tab), `\r`, `\0`, `\"` and `\\`.

`+` joins strings. If either side is a string, the other side is converted to its printed form. `*` repeats a string.

```rambo
print("score: " + 10, "ab" * 3);
```

## Operators

From lowest to highest precedence:

| Operators | Meaning |
|---|---|
| `\|\|` | or (short-circuit) |
| `&&` | and (short-circuit) |
| `==` `!=` | equality |
| `<` `<=` `>` `>=` | comparison |
| `+` `-` | add, subtract (and join strings/lists) |
| `*` `/` `%` | multiply, divide, remainder |
| `!` `-` | not, negate (prefix) |
| `f(x)` `a[i]` `a.b` | call, index, field |

`==` works on any two values; values of different types are never equal. Lists and maps compare by contents, and functions by identity.

`<` and friends compare two numbers or two strings (strings compare alphabetically, by Unicode code point). Comparing mismatched types is an error.

## Truthiness

Only `nil` and `false` count as false. Everything else is true, **including `0` and `""`**.

```rambo
if 0 { print("0 is truthy in Rambo"); }
```

`&&` and `||` return one of their operands (not necessarily a bool), which makes `||` handy for defaults:

```rambo
let nickname = nil;
print(nickname || "no nickname");
```
