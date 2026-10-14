//! Development-only authored metadata controls, without execution or installed APIs.
use std::io::{self, BufRead};
#[path = "support/semantic_fixture.rs"]
mod fixture;
fn main() {
    for line in io::stdin().lock().lines() {
        let input = serde_json::from_str(&line.expect("fixture stdin"))
            .expect("authored semantic fixture object");
        println!("{}", fixture::observe(input));
    }
}
