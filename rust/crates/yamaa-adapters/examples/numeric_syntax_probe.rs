//! Test-only JSON-lines replay through the same transport used by both hosts.
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        println!(
            "{}",
            yamaa_adapters::numeric_syntax::analyze_numeric(&line.unwrap()).unwrap()
        );
    }
}
