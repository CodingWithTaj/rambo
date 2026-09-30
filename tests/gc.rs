//! Garbage collector tests: cycles are freed, and live data survives collection.
mod common;
use common::run;

#[test]
fn cycles_are_collected() {
    let src = r#"
        for i in 0..20000 {
            let a = [];
            push(a, a);
            let m = {};
            m.self = m;
        }
        gc();
        let s = memory();
        print(s.live < 100, s.collections > 0, s.freed >= 40000);
    "#;
    assert_eq!(run(src), "true true true");
}

#[test]
fn closures_that_capture_themselves_are_collected() {
    let src = r#"
        for i in 0..10000 {
            fn loop_back() { return loop_back; }
        }
        gc();
        print(memory().live < 100);
    "#;
    assert_eq!(run(src), "true");
}

#[test]
fn reachable_data_survives_many_collections() {
    // build a 50,000-node linked list while plenty of garbage forces collections
    let src = r#"
        let head = nil;
        for i in 0..50000 {
            head = { value: i, next: head };
            let junk = [i, [i], { k: i }];
        }
        let total = 0;
        let node = head;
        while node != nil {
            total += node.value;
            node = node.next;
        }
        print(total, memory().collections > 5);
    "#;
    assert_eq!(run(src), "1249975000 true");
}

#[test]
fn captured_variables_survive_collection() {
    let src = r#"
        fn make() {
            let data = { hits: 0, log: [] };
            return fn(x) { data.hits += 1; push(data.log, x); return data; };
        }
        let f = make();
        for i in 0..30000 {
            let junk = [i];
            f(i % 3);
        }
        let d = f(9);
        print(d.hits, len(d.log), d.log[-1]);
    "#;
    assert_eq!(run(src), "30001 30001 9");
}

#[test]
fn memory_is_reused_after_collection() {
    let mut vm = rambo::vm::Vm::new();
    rambo::run_in(&mut vm, "for i in 0..200000 { let x = [i]; }").unwrap();
    // the heap should have stayed small, not grown to 200k objects
    assert!(vm.heap.live() < 10_000, "live objects: {}", vm.heap.live());
    assert!(vm.heap.collections > 10);
}

#[test]
fn temporary_strings_are_collected_and_live_ones_survive() {
    let src = r#"
        let keep = [];
        for i in 0..60000 {
            let temp = "temporary string number {i}";
            if i % 10000 == 0 { push(keep, temp); }
        }
        gc();
        print(memory().live < 500, len(keep), keep[3]);
        // a string rebuilt after its twin was freed must still compare equal
        print(keep[5] == "temporary string number " + str(50000));
    "#;
    assert_eq!(run(src), "true 6 temporary string number 30000\ntrue");
}

#[test]
fn map_string_keys_survive_collection() {
    let src = r#"
        let m = {};
        for i in 0..20000 {
            m["key" + str(i % 50)] = i;
            let junk = "junk" + str(i);
        }
        gc();
        print(len(m), m.key7, m["key" + "49"]);
    "#;
    assert_eq!(run(src), "50 19957 19999");
}
