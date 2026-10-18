//! In this outside graph the facade's YAML parser is the vendored copy, reached
//! by path. No `[patch]` is involved, and no published saphyr-parser joins the
//! graph (#2929).

use std::path::Path;
use std::process::Command;

use serde_json::Value;

#[test]
fn the_facade_links_the_vendored_parser_by_path() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--frozen",
            "--manifest-path",
        ])
        .arg(root.join("Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let parsers: Vec<&Value> = packages
        .iter()
        .filter(|package| package["name"].as_str().unwrap().contains("saphyr"))
        .collect();
    assert_eq!(parsers.len(), 1, "one YAML parser package: {parsers:?}");
    let parser = parsers[0];
    assert_eq!(parser["name"], "yamaa-saphyr-parser");
    assert!(parser["source"].is_null(), "a path package: {parser}");
    assert_eq!(
        Path::new(parser["manifest_path"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        root.join("../../vendor/saphyr-parser/Cargo.toml")
            .canonicalize()
            .unwrap(),
    );
    let adapters = packages
        .iter()
        .find(|package| package["name"] == "yamaa-adapters")
        .unwrap();
    let node = metadata["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == adapters["id"])
        .unwrap();
    assert!(
        node["dependencies"]
            .as_array()
            .unwrap()
            .contains(&parser["id"]),
        "yamaa-adapters resolves the vendored parser: {node}"
    );
}
