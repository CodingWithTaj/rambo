# Standard library

Every built-in function, grouped by topic. Functions marked *(prelude)* are written in Rambo itself and compiled when the VM starts; the rest are written in Rust. Run `rambo builtins` for a quick list.

Functions that take a list or string never change it unless they say so: `slice`, `reverse` and `sort` return new values, while `push`, `pop`, `insert` and `remove` change the collection in place.

## Core

| Function | Description |
|---|---|
| `print(...values)` | Print values separated by spaces, followed by a newline. |
| `type(value)` | The type as a string: `"nil"`, `"bool"`, `"number"`, `"string"`, `"list"`, `"map"` or `"function"`. |
| `str(value)` | Convert any value to its printed form. |
| `num(value)` | Convert a string (or bool) to a number. Returns `nil` if the string isn't a number. |
| `len(collection)` | Number of items in a list or map, or characters in a string. |
| `assert(condition, message?)` | Stop with an error if the condition is falsy. |
| `error(message)` | Stop the program with a runtime error. |
| `clock()` | Seconds since the program started, with sub-millisecond precision. |
| `gc()` | Ask the garbage collector to run before the next instruction. |
| `memory()` | Collector statistics as a map: `{ live, collections, freed }`. |

```rambo
print(type(3), str([1, 2]) + "!", num("42") + 1, num("woof"));
let start = clock();
print("elapsed seconds is a {type(clock() - start)}");
```

## Math

| Function | Description |
|---|---|
| `floor(x)` | Round down. |
| `ceil(x)` | Round up. |
| `round(x)` | Round to nearest; halves round away from zero. |
| `abs(x)` | Absolute value. |
| `sqrt(x)` | Square root. |
| `pow(base, exponent)` | Raise to a power. |
| `min(a, b)` | The smaller number. |
| `max(a, b)` | The larger number. |
| `random()` | A random number in `[0, 1)`. |
| `random_int(low, high)` | A random whole number from `low` to `high`, inclusive. |
| `seed(n)` | Seed the generator so random numbers repeat run to run. |
| `PI` | The constant π *(prelude)*. |

```rambo
seed(42);
print(floor(PI * 100) / 100, sqrt(2), pow(2, 16), random_int(1, 6));
```

## Lists

| Function | Description |
|---|---|
| `push(list, value)` | Add to the end. |
| `pop(list)` | Remove and return the last item. Error if empty. |
| `insert(list, index, value)` | Insert before `index` (negative counts from the end). |
| `remove(collection, index_or_key)` | Remove and return a list item by index, or a map entry by key. |
| `slice(list_or_string, start, end?)` | Copy of the part from `start` up to, not including, `end`. Negative indexes count from the end; out-of-range bounds are clamped. |
| `contains(collection, value)` | Whether a list has the value, a string has the substring, or a map has the key. |
| `index_of(list_or_string, value)` | Position of the first match, or `nil`. |
| `reverse(list_or_string)` | Reversed copy. |
| `sort(list)` | Sorted copy of a list of all numbers or all strings. Use `sort_by` for anything else. |
| `join(list, separator)` | Join items' printed forms into one string. |
| `range(end)`, `range(start, end, step?)` | List of numbers from `start` (default 0) up to, not including, `end`. `step` may be negative or fractional. |

```rambo
let xs = [5, 3, 8];
push(xs, 1);
insert(xs, 0, 9);
print(xs, sort(xs), slice(xs, 1, 3), index_of(xs, 8));
print(join(range(0, 10, 3), ", "));
```

## Higher-order functions (prelude)

| Function | Description |
|---|---|
| `map(list, f)` | New list of `f(x)` for each item. |
| `filter(list, keep)` | New list of the items where `keep(x)` is truthy. |
| `reduce(list, f, initial)` | Combine items left to right: `f(f(initial, x0), x1)`... |
| `each(list, f)` | Call `f(x)` for each item. |
| `find(list, matches)` | First item where `matches(x)` is truthy, or `nil`. |
| `any(list, test)` | Whether `test` is truthy for at least one item. |
| `all(list, test)` | Whether `test` is truthy for every item. |
| `sum(list)` | Total of a list of numbers (`0` if empty). |
| `sort_by(list, key)` | Sorted copy, ordered by `key(x)`. Stable: equal keys keep their order. |

```rambo
let dogs = [{name: "Rambo", age: 4}, {name: "Biscuit", age: 2}, {name: "Max", age: 9}];
print(map(sort_by(dogs, fn(d) { return d.age; }), fn(d) { return d.name; }));
print(sum(map(dogs, fn(d) { return d.age; })));
print(find(dogs, fn(d) { return d.age > 5; }).name);
```

## Strings

| Function | Description |
|---|---|
| `split(string, separator)` | List of pieces. An empty separator splits into characters. |
| `upper(string)` | Uppercase copy. |
| `lower(string)` | Lowercase copy. |
| `trim(string)` | Copy without leading and trailing whitespace. |
| `replace(string, from, to)` | Replace every occurrence. |
| `starts_with(string, prefix)` | Whether it begins with `prefix`. |
| `ends_with(string, suffix)` | Whether it ends with `suffix`. |
| `chars(string)` | List of characters. |
| `ord(char)` | Unicode code point of a one-character string. |
| `chr(code)` | One-character string for a code point. |

`len`, `slice`, `reverse`, `contains` and `index_of` (above) work on strings too.

```rambo
let s = "  Good Boy  ";
print(upper(trim(s)), split("a,b,c", ","), replace("woof woof", "oo", "u"));
print(ord("R"), chr(82), reverse("rambo"));
```

## Maps

| Function | Description |
|---|---|
| `keys(map)` | List of keys, in insertion order. |
| `values(map)` | List of values, in insertion order. |
| `items(map)` | List of `[key, value]` pairs. |
| `has(map, key)` | Whether the key is present. |

`len`, `remove` and `contains` (above) work on maps too.

```rambo
let ages = {rambo: 4, biscuit: 2};
for pair in items(ages) {
    print(pair[0], "->", pair[1]);
}
print(has(ages, "rambo"), keys(ages), values(ages));
```
