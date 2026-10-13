//! Development-only observations of authored custom schemas, without installed APIs.
#[path = "support/schema_declaration.rs"]
mod fixture;
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let (schema, document): (String, String) =
            serde_json::from_str(&line.expect("stdin")).expect("authored schema and document");
        println!("{}", fixture::observe(&schema, &document));
    }
}
