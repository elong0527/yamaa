//! Development-only observations of authored custom schemas, without installed APIs.
#[path = "support/schema_declaration.rs"]
mod fixture;
use std::io::{self, BufRead};
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Request {
    Default((fixture::SchemaInput, String)),
    Named((fixture::SchemaInput, String, Option<String>)),
}
fn main() {
    for line in io::stdin().lock().lines() {
        let request: Request =
            serde_json::from_str(&line.expect("stdin")).expect("authored schema and document");
        let (schema, document, root) = match request {
            Request::Default((schema, document)) => (schema, document, None),
            Request::Named((schema, document, root)) => (schema, document, root),
        };
        println!(
            "{}",
            fixture::observe_input(schema, &document, root.as_deref())
        );
    }
}
