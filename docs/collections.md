# Collections

## Lists

A list is an ordered, growable sequence of any values.

```rambo
let tricks = ["sit", "shake"];
push(tricks, "roll over");
print(tricks, len(tricks));
print(tricks[0], tricks[-1]);   // negative indexes count from the end
tricks[1] = "paw";
print(tricks);
```

Indexing past the end is an error, not `nil`. `+` joins two lists into a new one. `slice`, `insert`, `remove`, `pop`, `reverse`, `sort`, `contains`, `index_of` and `join` are covered in the [standard library](stdlib.md).

## Maps

A map stores values by key. Keys can be strings, numbers, booleans or `nil`. Maps remember the order keys were added, so printing and looping are predictable.

```rambo
let dog = {
    name: "Rambo",
    age: 4,
    "favourite toy": "tennis ball",
};
print(dog);
```

Inside a map literal, a bare name like `name:` means the string key `"name"`. To use a variable or expression as a key, wrap it in brackets: `{[key]: value}`.

Read and write entries with brackets, or with a dot when the key is a valid name:

```rambo
let dog = {name: "Rambo"};
dog.age = 4;
dog["breed"] = "mixed";
dog.age += 1;
print(dog.name, dog.age, dog["breed"]);
```

Looking up a key that isn't there gives `nil`. Use `has(map, key)` to tell "missing" apart from "stored `nil`", and `remove(map, key)` to delete an entry.

```rambo
let counts = {};
for word in split("the dog saw the ball", " ") {
    if has(counts, word) { counts[word] += 1; } else { counts[word] = 1; }
}
print(counts);
print(keys(counts));
```

## Lists and maps are shared

Assigning a list or map to another variable doesn't copy it. Both names refer to the same object, so a change through one is visible through the other. The same happens when you pass one to a function.

```rambo
let a = [1, 2];
let b = a;
push(b, 3);
print(a);   // [1, 2, 3]
```

To copy a list, use `slice(xs, 0)`.

## Garbage collection

You never free memory yourself. Rambo's garbage collector finds lists, maps, functions and strings that the program can no longer reach and reuses their memory, including objects that refer to each other in a cycle.

```rambo
for i in 0..20000 {
    let node = {next: nil};
    node.next = node;   // a cycle: unreachable after each iteration
}
gc();
print(memory());
```

See [How it works](internals.md#the-garbage-collector) for the details.
