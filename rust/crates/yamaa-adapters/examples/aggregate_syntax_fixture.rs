//! Test fixture observation only; this is not an installed request protocol.
#[path = "support/aggregate_syntax.rs"]
mod fixture;
use std::io::{self, BufRead};
fn main() {
    let spans = std::env::args().any(|argument| argument == "--spans");
    for line in io::stdin().lock().lines() {
        let text: String = serde_json::from_str(&line.expect("stdin")).expect("fixture expression");
        println!(
            "{}",
            if spans {
                fixture::observe_with_spans(&text)
            } else {
                fixture::observe(&text)
            }
        );
    }
}
