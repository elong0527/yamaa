//! Development-only observations of the actual descriptor and snapshot owner.
#[path = "support/resource_capture_fixture.rs"]
mod fixture;
use std::io::{self, BufRead};
fn main() {
    let mut owner = fixture::Owner::default();
    for line in io::stdin().lock().lines() {
        let request =
            serde_json::from_str(&line.expect("stdin")).expect("authored resource request");
        println!("{}", owner.observe(request));
    }
}
