//! Fuzz tests: throw thousands of random and mangled programs at the
//! compiler and VM. Whatever the input, Rambo must report an error or run,
//! never crash (panic).

use std::panic;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str { xs[(self.next() % xs.len() as u64) as usize] }
}

const PIECES: &[&str] = &[
    "let", "fn", "return", "if", "else", "while", "for", "in", "break", "continue", "true", "false", "nil",
    "x", "y", "f", "xs", "m", "print", "len", "push", "map", "(", ")", "{", "}", "[", "]", ",", ";", ":",
    ".", "..", "+", "-", "*", "/", "%", "+=", "=", "==", "!=", "<", ">", "&&", "||", "!", "0", "1", "2.5",
    "\"s\"", "\"a{x}b\"", "\n", " ",
];

const SEEDS: &[&str] = &[
    "let x = 1; let xs = [1, 2, 3]; for i in 0..3 { x += xs[i]; } print(x);",
    "fn f(a) { if a < 1 { return 0; } return f(a - 1) + 1; } print(f(5));",
    "let m = {a: 1, b: [2]}; for k in m { print(k, m[k]); } m.c = fn(x) { return x; };",
    "fn outer() { let n = 0; return fn() { n += 1; return n; }; } let g = outer(); g(); print(g());",
    "let s = \"hi {1 + 2}\"; while len(s) < 20 { s += \"!\"; } print(upper(s));",
];

fn check(src: &str) {
    let result = panic::catch_unwind(|| {
        let _ = rambo::run_source(src, Some(20_000));
        let _ = rambo::disassemble_source(src);
    });
    assert!(result.is_ok(), "rambo panicked on input:\n{}", src);
}

#[test]
fn random_token_soup_never_panics() {
    panic::set_hook(Box::new(|_| {}));
    let mut rng = Rng(0x2545F4914F6CDD1D);
    for _ in 0..3000 {
        let n = 1 + rng.next() % 40;
        let src: Vec<&str> = (0..n).map(|_| rng.pick(PIECES)).collect();
        check(&src.join(" "));
    }
    let _ = panic::take_hook();
}

#[test]
fn mutated_valid_programs_never_panic() {
    panic::set_hook(Box::new(|_| {}));
    let mut rng = Rng(0x9E3779B97F4A7C15);
    for _ in 0..3000 {
        let base: Vec<char> = rng.pick(SEEDS).chars().collect();
        let mut src = base.clone();
        for _ in 0..(1 + rng.next() % 4) {
            if src.is_empty() { break; }
            let at = (rng.next() % src.len() as u64) as usize;
            match rng.next() % 3 {
                0 => { src.remove(at); }
                1 => { let c = base[(rng.next() % base.len() as u64) as usize]; src.insert(at, c); }
                _ => { let piece: Vec<char> = rng.pick(PIECES).chars().collect(); for (k, c) in piece.into_iter().enumerate() { src.insert(at + k, c); } }
            }
        }
        check(&src.iter().collect::<String>());
    }
    let _ = panic::take_hook();
}
