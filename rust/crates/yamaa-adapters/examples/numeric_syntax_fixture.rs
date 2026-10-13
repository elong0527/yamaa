//! Test fixture observation only; this is not an installed request protocol.
#[path = "support/numeric_syntax.rs"]
mod fixture;
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let text: String = serde_json::from_str(&line.expect("stdin")).expect("fixture expression");
        println!("{}", fixture::observe(&text));
    }
}
