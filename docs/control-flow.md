# Control flow

## if / else

Conditions don't need parentheses, but the braces are required.

```rambo
let age = 4;
if age < 1 {
    print("puppy");
} else if age < 8 {
    print("adult");
} else {
    print("senior");
}
```

## while

```rambo
let n = 1;
while n < 100 {
    n *= 2;
}
print(n);
```

## for over a range

`for i in start..end` counts from `start` up to, **but not including**, `end`. Both ends are evaluated once, before the loop starts.

```rambo
for i in 1..4 {
    print("round {i}");
}
```

Each iteration gets its own fresh copy of the loop variable. Changing it inside the body doesn't affect the loop, and closures created in the body each remember their own value (see [Functions](functions.md)).

To count by other steps, or backwards, loop over `range`:

```rambo
for i in range(10, 0, -3) {
    print(i);
}
```

## for over a collection

`for x in collection` loops over the items of a list, the characters of a string, or the keys of a map (in insertion order).

```rambo
for c in "dog" { print(c); }

let ages = {rambo: 4, biscuit: 2};
for name in ages {
    print("{name} is {ages[name]}");
}
```

## break and continue

`break` leaves the innermost loop immediately; `continue` skips to its next iteration.

```rambo
for i in 0..10 {
    if i % 2 == 0 { continue; }
    if i > 7 { break; }
    print(i);
}
```
