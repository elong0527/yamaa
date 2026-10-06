//! Test-only JSON-lines replay through the shared predicate syntax service.
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        println!(
            "{}",
            yamaa_adapters::predicate_syntax::analyze_predicate(&line.unwrap()).unwrap()
        );
    }
}
