//! Development-only R022 observation probe; not an installed host API.
use serde::Deserialize;
use serde_json::json;
use std::io::{self, BufRead};
use yamaa_core::regex::{CompileError, CompileLimits, MatchLimits, Pattern};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    pattern: String,
    subject: String,
}
/// Observe actual core results for development-only JSON-lines requests.
fn main() {
    for line in io::stdin().lock().lines() {
        let line = line.expect("stdin");
        let request: Request = serde_json::from_str(&line).expect("request");
        let outcome = match Pattern::compile(&request.pattern, CompileLimits::default()) {
            Err(CompileError::Invalid { .. }) => json!({"accepted":false}),
            Err(error) => json!({"policy_error":format!("{error:?}")}),
            Ok(pattern) => {
                match (
                    pattern.search(&request.subject, MatchLimits::default()),
                    pattern.full_match(&request.subject, MatchLimits::default()),
                ) {
                    (Ok(found), Ok(full)) => {
                        json!({"accepted":true,"group_count":pattern.group_count(),"full_match":full.is_some(),"match":found.map(|found|found.groups)})
                    }
                    (a, b) => json!({"policy_error":format!("{a:?} / {b:?}")}),
                }
            }
        };
        println!("{outcome}");
    }
}
