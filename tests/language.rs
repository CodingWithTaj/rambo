//! End-to-end tests: each runs a Rambo program and checks what it prints.
mod common;
use common::{run, run_err};

#[test]
fn arithmetic_and_precedence() {
    assert_eq!(run("print(1 + 2 * 3);"), "7");
    assert_eq!(run("print((1 + 2) * 3);"), "9");
    assert_eq!(run("print(10 - 4 - 3);"), "3");
    assert_eq!(run("print(7 / 2);"), "3.5");
    assert_eq!(run("print(-7 % 3);"), "2");
    assert_eq!(run("print(-(2 + 3) * 2);"), "-10");
    assert_eq!(run("print(1_000_000 + 1);"), "1000001");
}

#[test]
fn comparison_and_logic() {
    assert_eq!(run("print(1 < 2, 2 <= 2, 3 > 4, 4 >= 5);"), "true true false false");
    assert_eq!(run("print(\"a\" < \"b\");"), "true");
    assert_eq!(run("print(!true, !nil, !0);"), "false true false");
    assert_eq!(run("print(nil || \"default\");"), "default");
    assert_eq!(run("print(1 && 2);"), "2");
}

#[test]
fn logical_operators_short_circuit() {
    let src = r#"
        fn boom() { print("should not run"); return true; }
        print(false && boom());
        print(true || boom());
    "#;
    assert_eq!(run(src), "false\ntrue");
}

#[test]
fn comments() {
    assert_eq!(run("// line\nprint(1); /* block\n spanning lines */ print(2);"), "1\n2");
}

#[test]
fn strings() {
    assert_eq!(run(r#"print("a" + "b" + 1);"#), "ab1");
    assert_eq!(run(r#"print("ab" * 3);"#), "ababab");
    assert_eq!(run(r#"print(len("héllo"), "héllo"[1], "abc"[-1]);"#), "5 é c");
    assert_eq!(run(r#"print("tab\there");"#), "tab\there");
}

#[test]
fn string_interpolation() {
    assert_eq!(run(r#"let n = "Rambo"; print("hi {n}!");"#), "hi Rambo!");
    assert_eq!(run(r#"print("{1 + 2} and {len([1, 2])}");"#), "3 and 2");
    assert_eq!(run(r#"let d = {name: "x"}; print("name: {d.name}");"#), "name: x");
    assert_eq!(run(r#"print("nested {"inner {1}"}");"#), "nested inner 1");
    assert_eq!(run(r#"print("literal \{braces\}");"#), "literal {braces}");
    assert_eq!(run(r#"print("{[1, "a"]}");"#), "[1, \"a\"]");
}

#[test]
fn compound_assignment() {
    assert_eq!(run("let x = 10; x += 5; x -= 3; x *= 2; x /= 4; x %= 4; print(x);"), "2");
    assert_eq!(run("let xs = [1, 2]; xs[0] += 10; xs[-1] *= 3; print(xs);"), "[11, 6]");
    assert_eq!(run("let s = \"a\"; s += \"b\"; print(s);"), "ab");
    assert_eq!(run("let m = {n: 1}; m.n += 1; m[\"n\"] += 1; print(m.n);"), "3");
}

#[test]
fn lists() {
    assert_eq!(run("let xs = [1, 2, 3]; push(xs, 4); print(xs, len(xs));"), "[1, 2, 3, 4] 4");
    assert_eq!(run("let xs = [1, 2, 3]; print(xs[-1], pop(xs), xs);"), "3 3 [1, 2]");
    assert_eq!(run("let xs = [0, 0]; xs[1] = \"x\"; print(xs);"), "[0, \"x\"]");
    assert_eq!(run("print([1, [2, 3]] == [1, [2, 3]]);"), "true");
    assert_eq!(run("print([1] + [2]);"), "[1, 2]");
    assert_eq!(run("print([1, 2,]);"), "[1, 2]");
}

#[test]
fn lists_are_shared_references() {
    assert_eq!(run("let a = [1]; let b = a; push(b, 2); print(a);"), "[1, 2]");
}

#[test]
fn maps() {
    assert_eq!(run("let m = {a: 1, \"b c\": 2, 3: \"three\"}; print(m);"), "{ a: 1, \"b c\": 2, 3: \"three\" }");
    assert_eq!(run("let m = {}; m.x = 1; m[\"y\"] = 2; print(m.x + m.y, len(m));"), "3 2");
    assert_eq!(run("let m = {a: 1}; print(m.missing);"), "nil");
    assert_eq!(run("let k = \"dyn\"; let m = {[k]: 5}; print(m.dyn);"), "5");
    assert_eq!(run("print({a: 1, b: [2]} == {b: [2], a: 1});"), "true");
    assert_eq!(run("print({});"), "{}");
}

#[test]
fn map_iteration_is_in_insertion_order() {
    let src = "let m = {z: 1, a: 2, m: 3}; let out = []; for k in m { push(out, k); } print(out);";
    assert_eq!(run(src), "[\"z\", \"a\", \"m\"]");
}

#[test]
fn nested_data() {
    let src = r#"
        let dogs = [{name: "Rambo", tricks: ["sit"]}];
        push(dogs[0].tricks, "shake");
        dogs[0].tricks[0] = "stay";
        print(dogs[0].tricks);
    "#;
    assert_eq!(run(src), "[\"stay\", \"shake\"]");
}

#[test]
fn if_else_chains() {
    let src = r#"
        fn grade(n) {
            if n >= 90 { return "A"; }
            else if n >= 80 { return "B"; }
            else { return "C"; }
        }
        print(grade(95), grade(85), grade(10));
    "#;
    assert_eq!(run(src), "A B C");
}

#[test]
fn loops_with_break_and_continue() {
    let src = r#"
        let out = [];
        for i in 0..10 {
            let doubled = i * 2;
            if i % 2 == 0 { continue; }
            if i > 7 { break; }
            push(out, doubled);
        }
        print(out);
        let n = 0;
        while true {
            let step = 1;
            n += step;
            if n == 5 { break; }
        }
        print(n);
    "#;
    assert_eq!(run(src), "[2, 6, 10, 14]\n5");
}

#[test]
fn break_and_continue_in_nested_loops_keep_the_stack_balanced() {
    let src = r#"
        let pairs = 0;
        for i in 0..50 {
            let a = i;
            for j in 0..50 {
                let b = j;
                if b == 3 { break; }
                if b == 1 { continue; }
                pairs += 1;
            }
        }
        print(pairs);
    "#;
    assert_eq!(run(src), "100");
}

#[test]
fn for_each_over_strings_and_lists() {
    assert_eq!(run("let s = \"\"; for c in \"abc\" { s = c + s; } print(s);"), "cba");
    assert_eq!(run("let t = 0; for x in [1, 2, 3] { t += x; } print(t);"), "6");
}

#[test]
fn recursion() {
    assert_eq!(run("fn fact(n) { if n <= 1 { return 1; } return n * fact(n - 1); } print(fact(10));"), "3628800");
}

#[test]
fn global_functions_can_be_mutually_recursive() {
    let src = r#"
        fn is_even(n) { if n == 0 { return true; } return is_odd(n - 1); }
        fn is_odd(n) { if n == 0 { return false; } return is_even(n - 1); }
        print(is_even(10), is_odd(7));
    "#;
    assert_eq!(run(src), "true true");
}

#[test]
fn local_functions_can_recurse() {
    let src = r#"
        fn outer() {
            fn countdown(n) { if n == 0 { return "done"; } return countdown(n - 1); }
            return countdown(5);
        }
        print(outer());
    "#;
    assert_eq!(run(src), "done");
}

#[test]
fn closures_capture_and_mutate() {
    let src = r#"
        fn counter() {
            let n = 0;
            return fn() { n += 1; return n; };
        }
        let a = counter();
        let b = counter();
        a(); a();
        print(a(), b());
    "#;
    assert_eq!(run(src), "3 1");
}

#[test]
fn closures_share_captured_variables() {
    let src = r#"
        fn pair() {
            let value = 0;
            let get = fn() { return value; };
            let set = fn(v) { value = v; };
            return [get, set];
        }
        let p = pair();
        p[1](42);
        print(p[0]());
    "#;
    assert_eq!(run(src), "42");
}

#[test]
fn closures_see_later_changes_to_outer_variables() {
    let src = r#"
        fn f() {
            let x = 1;
            let get = fn() { return x; };
            x = 2;
            return get;
        }
        print(f()());
    "#;
    assert_eq!(run(src), "2");
}

#[test]
fn closures_reach_several_levels_up() {
    let src = r#"
        fn level1() {
            let x = "from level 1";
            fn level2() {
                fn level3() { return x; }
                return level3;
            }
            return level2();
        }
        print(level1()());
    "#;
    assert_eq!(run(src), "from level 1");
}

#[test]
fn each_loop_iteration_gets_a_fresh_variable() {
    let src = r#"
        let fs = [];
        for i in 0..3 { push(fs, fn() { return i; }); }
        for x in ["a", "b"] { push(fs, fn() { return x; }); }
        print(map(fs, fn(f) { return f(); }));
    "#;
    assert_eq!(run(src), "[0, 1, 2, \"a\", \"b\"]");
}

#[test]
fn closures_capturing_block_locals_in_a_loop() {
    let src = r#"
        let fs = [];
        let i = 0;
        while i < 3 {
            let j = i * 10;
            push(fs, fn() { return j; });
            i += 1;
        }
        print(fs[0](), fs[1](), fs[2]());
    "#;
    assert_eq!(run(src), "0 10 20");
}

#[test]
fn shadowing_in_blocks() {
    let src = r#"
        fn f() {
            let x = 1;
            {
                let x = x + 10;
                print(x);
            }
            print(x);
        }
        f();
    "#;
    assert_eq!(run(src), "11\n1");
}

#[test]
fn top_level_block_locals() {
    assert_eq!(run("let g = 1; { let l = g + 1; print(l); } print(g);"), "2\n1");
}

#[test]
fn implicit_return_is_nil() {
    assert_eq!(run("fn f() { } print(f());"), "nil");
    assert_eq!(run("fn f() { return; } print(f());"), "nil");
}

#[test]
fn values_print_nicely() {
    assert_eq!(run("print(0.1 + 0.2, 10 / 4, pow(10, 400), -pow(10, 400));"), "0.30000000000000004 2.5 inf -inf");
    assert_eq!(run("fn named() {} print(named, print);"), "<fn named> <builtin print>");
    assert_eq!(run("let xs = [1]; push(xs, xs); print(xs);"), "[1, <cycle>]");
}

// ---------------------------------------------------------------- errors

#[test]
fn syntax_errors_point_at_the_problem() {
    let e = run_err("let x = 1;\nlet y = ;");
    assert!(e.contains("line 2, column 9") && e.contains("expected an expression, found ';'"), "{}", e);
    assert!(e.contains("2 | let y = ;") && e.contains("  |         ^"), "{}", e);

    let e = run_err("print(\"never closed);");
    assert!(e.contains("unterminated string"), "{}", e);

    let e = run_err("fn f( { }");
    assert!(e.contains("parameter name"), "{}", e);
}

#[test]
fn missing_semicolon_points_at_the_end_of_the_line() {
    let e = run_err("let x = 1\nprint(x);");
    assert!(e.contains("missing ';'") && e.contains("line 1, column 10"), "{}", e);
}

#[test]
fn compile_errors() {
    assert!(run_err("break;").contains("inside a loop"));
    assert!(run_err("return 1;").contains("inside a function"));
    assert!(run_err("1 + 2 = 3;").contains("can only assign"));
}

#[test]
fn runtime_errors_have_carets_and_stack_traces() {
    let e = run_err("fn inner() { return 1 + nil; }\nfn outer() { return inner(); }\nouter();");
    assert!(e.contains("can't add number and nil"), "{}", e);
    assert!(e.contains("line 1, column 23"), "{}", e);
    assert!(e.contains("at inner (line 1)") && e.contains("at outer (line 2)") && e.contains("at <script> (line 3)"), "{}", e);
}

#[test]
fn runtime_error_kinds() {
    assert!(run_err("print(undefined_thing);").contains("'undefined_thing' is not defined"));
    assert!(run_err("missing = 1;").contains("declare it first"));
    assert!(run_err("print([1, 2][5]);").contains("out of range"));
    assert!(run_err("print(1 / 0 % 0);").contains("division by zero"));
    assert!(run_err("fn f(a) {} f(1, 2);").contains("f() expects 1 argument, got 2"));
    assert!(run_err("let x = 5; x();").contains("can only call functions"));
    assert!(run_err("for x in 5 { }").contains("can't loop over a number"));
    assert!(run_err("let m = {[[1]]: 2};").contains("map keys must be"));
    assert!(run_err("print([1][\"a\"]);").contains("indexed by number"));
    assert!(run_err("let s = \"abc\"; s[0] = \"x\";").contains("can't be changed in place"));
    assert!(run_err("print(len(5));").contains("len(): expected a list, map or string, got number"));
    assert!(run_err("push(1);").contains("push() expects 2 arguments, got 1"));
}

#[test]
fn errors_inside_library_functions_name_them() {
    let e = run_err("map([1, 2], 5);");
    assert!(e.contains("inside map()") && e.contains("can only call functions"), "{}", e);
    assert!(e.contains("at <script> (line 1)"), "{}", e);
}

#[test]
fn error_and_assert() {
    assert!(run_err("error(\"custom failure\");").contains("runtime error: custom failure"));
    assert!(run_err("assert(1 == 2, \"math is broken\");").contains("assertion failed: math is broken"));
    assert_eq!(run("assert(true); print(\"ok\");"), "ok");
}

#[test]
fn deep_recursion_is_caught() {
    assert!(run_err("fn f(n) { return f(n + 1); } f(0);").contains("stack overflow"));
}

#[test]
fn infinite_loops_hit_the_step_limit() {
    assert!(run_err("while true { }").contains("infinite loop"));
}

#[test]
fn disassembler_shows_functions_and_captures() {
    let text = rambo::disassemble_source("fn outer() { let n = 1; return fn() { return n; }; }").unwrap();
    assert!(text.contains("== <script>") && text.contains("== outer (params: 0"), "{}", text);
    assert!(text.contains("CLOSURE") && text.contains("captures n"), "{}", text);
    assert!(text.contains("GET_UPVALUE") && text.contains("; n"), "{}", text);
    assert!(text.contains("DEFINE_GLOBAL") && text.contains("; outer"), "{}", text);
}

#[test]
fn strings_built_at_runtime_equal_literals() {
    assert_eq!(run(r#"let a = "ra"; print(a + "mbo" == "rambo", "x" * 3 == "xxx", upper("a") == "A");"#), "true true true");
    assert_eq!(run(r#"let m = {rambo: 1}; print(m["ram" + "bo"]);"#), "1");
}
