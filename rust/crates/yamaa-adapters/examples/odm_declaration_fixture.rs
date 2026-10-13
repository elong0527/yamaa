//! Development-only ODM declaration and immutable metadata observations.
#[path = "support/odm_declaration.rs"]
mod fixture;
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let (source, headers): (String, Vec<Vec<String>>) =
            serde_json::from_str(&line.expect("stdin")).expect("authored fixture and headers");
        println!("{}", fixture::observe(&source, &headers));
    }
}
