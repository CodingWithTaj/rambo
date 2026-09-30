pub fn run(src: &str) -> String {
    let r = rambo::run_source(src, Some(50_000_000));
    if let Some(e) = r.error {
        panic!("program failed:\n{}\n--- output so far ---\n{}", e.render(src), r.output);
    }
    r.output.trim_end().to_string()
}

pub fn run_err(src: &str) -> String {
    let r = rambo::run_source(src, Some(1_000_000));
    let e = r.error.expect("expected an error, but the program succeeded");
    e.render(src)
}
