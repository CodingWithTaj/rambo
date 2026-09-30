//! Every built-in function, plus a check that the docs cover all of them.
mod common;
use common::{run, run_err};

#[test]
fn core() {
    assert_eq!(run("print(type(1), type(\"s\"), type([]), type({}), type(nil), type(true), type(print), type(map));"),
        "number string list map nil bool function function");
    assert_eq!(run("print(str(12) + str([1]), num(\"42\") + 1, num(\"nope\"), num(true));"), "12[1] 43 nil 1");
    assert_eq!(run("print(len(\"héllo\"), len([1, 2]), len({a: 1}));"), "5 2 1");
    assert_eq!(run("let t = clock(); print(type(t), t >= 0);"), "number true");
}

#[test]
fn math() {
    assert_eq!(run("print(floor(3.7), ceil(3.2), round(2.5), round(-2.5), abs(-2));"), "3 4 3 -3 2");
    assert_eq!(run("print(sqrt(16), pow(2, 10), min(1, 2), max(1, 2));"), "4 1024 1 2");
    assert_eq!(run("print(floor(PI * 100));"), "314");
}

#[test]
fn random_is_repeatable_with_a_seed() {
    let src = r#"
        seed(7);
        let a = [random(), random_int(1, 6)];
        seed(7);
        let b = [random(), random_int(1, 6)];
        print(a == b, a[0] >= 0 && a[0] < 1, a[1] >= 1 && a[1] <= 6);
    "#;
    assert_eq!(run(src), "true true true");
    assert_eq!(run("let ok = true; for i in 0..1000 { let n = random_int(-2, 2); if n < -2 || n > 2 || floor(n) != n { ok = false; } } print(ok);"), "true");
}

#[test]
fn list_functions() {
    assert_eq!(run("let xs = [1, 3]; insert(xs, 1, 2); insert(xs, -1, 9); print(xs);"), "[1, 2, 9, 3]");
    assert_eq!(run("let xs = [1, 2, 3]; print(remove(xs, 0), remove(xs, -1), xs);"), "1 3 [2]");
    assert_eq!(run("print(slice([1, 2, 3, 4], 1, 3), slice([1, 2, 3], -2), slice([1, 2], 5));"), "[2, 3] [2, 3] []");
    assert_eq!(run("print(contains([1, [2]], [2]), index_of([5, 6, 7], 7), index_of([1], 9));"), "true 2 nil");
    assert_eq!(run("print(reverse([1, 2, 3]), sort([3, 1, 2]), sort([\"b\", \"a\"]));"), "[3, 2, 1] [1, 2, 3] [\"a\", \"b\"]");
    assert_eq!(run("print(join([1, \"a\", true], \"-\"));"), "1-a-true");
    assert_eq!(run("print(range(3), range(2, 5), range(10, 0, -3), range(0, 1, 0.5));"), "[0, 1, 2] [2, 3, 4] [10, 7, 4, 1] [0, 0.5]");
    assert!(run_err("sort([1, \"a\"]);").contains("all numbers or all strings"));
    assert!(run_err("pop([]);").contains("empty list"));
}

#[test]
fn string_functions() {
    assert_eq!(run("print(split(\"a,b,,c\", \",\"), split(\"hi\", \"\"));"), "[\"a\", \"b\", \"\", \"c\"] [\"h\", \"i\"]");
    assert_eq!(run("print(upper(\"Rambo\"), lower(\"Rambo\"), trim(\"  x  \") + \"|\");"), "RAMBO rambo x|");
    assert_eq!(run("print(replace(\"a-b-c\", \"-\", \"+\"), starts_with(\"rambo\", \"ram\"), ends_with(\"rambo\", \"bo\"));"), "a+b+c true true");
    assert_eq!(run("print(chars(\"héy\"), ord(\"A\"), chr(97), slice(\"héllo\", 1, 3), reverse(\"abc\"));"), "[\"h\", \"é\", \"y\"] 65 a él cba");
    assert_eq!(run("print(contains(\"rambo\", \"mb\"), index_of(\"héllo\", \"llo\"));"), "true 2");
}

#[test]
fn map_functions() {
    let src = r#"
        let m = {b: 2, a: 1};
        print(keys(m), values(m), items(m));
        print(has(m, "a"), has(m, "z"), remove(m, "b"), m);
        print(contains(m, "a"));
    "#;
    assert_eq!(run(src), "[\"b\", \"a\"] [2, 1] [[\"b\", 2], [\"a\", 1]]\ntrue false 2 { a: 1 }\ntrue");
}

#[test]
fn prelude_functions() {
    let src = r#"
        let xs = [3, 1, 2];
        print(map(xs, fn(x) { return x * 10; }));
        print(filter(xs, fn(x) { return x > 1; }));
        print(reduce(xs, fn(a, x) { return a + x; }, 100));
        each(xs, fn(x) { print("item", x); });
        print(find(xs, fn(x) { return x < 3; }), find(xs, fn(x) { return x > 9; }));
        print(any(xs, fn(x) { return x == 2; }), all(xs, fn(x) { return x > 0; }));
        print(sum(xs), sum([]));
        print(sort_by(["ccc", "a", "bb", "dd"], fn(s) { return len(s); }));
    "#;
    assert_eq!(run(src), "[30, 10, 20]\n[3, 2]\n106\nitem 3\nitem 1\nitem 2\n1 nil\ntrue true\n6 0\n[\"a\", \"bb\", \"dd\", \"ccc\"]");
}

#[test]
fn every_builtin_is_documented() {
    let docs = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/stdlib.md")).unwrap();
    let mut missing = vec![];
    for n in rambo::natives::NATIVES {
        if !docs.contains(&format!("`{}(", n.name)) { missing.push(n.name); }
    }
    for name in rambo::natives::PRELUDE_NAMES {
        if !docs.contains(&format!("`{}", name)) { missing.push(name); }
    }
    assert!(missing.is_empty(), "docs/stdlib.md is missing: {:?}", missing);
}

#[test]
fn prelude_names_list_is_accurate() {
    let vm = rambo::vm::Vm::new();
    for name in rambo::natives::PRELUDE_NAMES {
        assert!(vm.globals.lookup(name).is_some(), "prelude doesn't define {}", name);
    }
}
