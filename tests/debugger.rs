//! The step-through debugger used by the playground.
use rambo::debug::{Session, StepMode};

const SRC: &str = "fn add(a, b) {
    let sum = a + b;
    return sum;
}
let x = 1;
let y = add(x, 2);
for i in 0..3 {
    y += i;
}
print(y);
";

fn line(s: &Session) -> u32 { s.vm.position().unwrap().0 }

#[test]
fn step_into_follows_calls() {
    let mut s = Session::start(SRC, None).unwrap();
    assert_eq!(line(&s), 1); // defining add
    s.step(StepMode::Into);
    assert_eq!(line(&s), 5);
    s.step(StepMode::Into);
    assert_eq!(line(&s), 6);
    s.step(StepMode::Into);
    assert_eq!(line(&s), 2, "should enter add()");
    let state = s.state_json(SRC);
    assert!(state.contains("\"name\":\"add\"") && state.contains("[\"a\",\"1\"]") && state.contains("[\"b\",\"2\"]"), "{}", state);
    s.step(StepMode::Into);
    assert_eq!(line(&s), 3);
    assert!(s.state_json(SRC).contains("[\"sum\",\"3\"]"));
}

#[test]
fn step_over_skips_calls() {
    let mut s = Session::start(SRC, None).unwrap();
    s.step(StepMode::Over);
    s.step(StepMode::Over);
    assert_eq!(line(&s), 6);
    s.step(StepMode::Over);
    assert_eq!(line(&s), 7, "should not enter add()");
    assert!(s.state_json(SRC).contains("[\"y\",\"3\"]"));
}

#[test]
fn step_out_returns_to_the_caller() {
    let mut s = Session::start(SRC, None).unwrap();
    for _ in 0..3 { s.step(StepMode::Into); }
    assert_eq!(line(&s), 2);
    s.step(StepMode::Out);
    assert_eq!(line(&s), 6);
    assert_eq!(s.vm.frames.len(), 1);
}

#[test]
fn continue_stops_at_breakpoints_each_iteration() {
    let mut s = Session::start(SRC, None).unwrap();
    s.breakpoints = vec![8];
    let mut hits = vec![];
    loop {
        s.step(StepMode::Continue);
        if s.finished { break; }
        let st = s.state_json(SRC);
        let i = st.split("[\"i\",\"").nth(1).unwrap().split('"').next().unwrap().to_string();
        hits.push(i);
    }
    assert_eq!(hits, vec!["0", "1", "2"]);
    assert!(s.state_json(SRC).contains("\"output\":\"6\\n\""));
}

#[test]
fn stepping_skips_library_code() {
    let src = "let xs = map([1, 2], fn(x) {\n    return x * 2;\n});\nprint(xs);";
    let mut s = Session::start(src, None).unwrap();
    s.step(StepMode::Into);
    // map() is library code, so stepping lands in the user's callback, not inside map
    assert_eq!(line(&s), 2);
    assert!(!s.vm.frames.last().unwrap().proto.is_prelude);
}

#[test]
fn runtime_errors_end_the_session_with_a_report() {
    let src = "let a = 1;\nlet b = a + nil;";
    let mut s = Session::start(src, None).unwrap();
    s.step(StepMode::Continue);
    assert!(s.finished);
    let st = s.state_json(src);
    assert!(st.contains("\"status\":\"error\"") && st.contains("can't add number and nil") && st.contains("\"errorLine\":2"), "{}", st);
}

#[test]
fn single_instruction_steps_show_the_opcode() {
    let mut s = Session::start("let x = 40 + 2;", None).unwrap();
    assert!(s.state_json("").contains("\"op\":\"CONST 0  ; 40\""));
    s.step(StepMode::Instruction);
    s.step(StepMode::Instruction);
    assert!(s.state_json("").contains("\"op\":\"ADD\""));
    assert!(s.state_json("").contains("\"stack\":[\"40\",\"2\"]"));
}
