//! Development-only logical-source metadata; no engine or publication is run.
#[path = "support/source_catalog_fixture.rs"]
mod fixture;
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let request =
            serde_json::from_str(&line.expect("stdin")).expect("authored metadata request");
        println!("{}", fixture::observe(request));
    }
}
