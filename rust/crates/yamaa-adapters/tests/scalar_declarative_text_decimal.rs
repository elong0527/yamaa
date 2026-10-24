#![cfg(feature = "yaml")]
//! `str_decimal` (TEXT-15) through file application: the reader of
//! `fixtures/text_decimal_reference.json`, whose runs give the text each value
//! is written as or the issues a declaration is refused with, and CSV display
//! precision (CSV-9) writing the reference's tie and near-tie values the same
//! way.
use crate::support::study::{issues, Study};
use yamaa_adapters::json;
use yamaa_core::json::Value as Json;

fn reference() -> Json {
    yamaa_adapters::json_wire::decode(include_str!("fixtures/text_decimal_reference.json")).unwrap()
}

/// One run of the reference: `TEXT` writes the run's `source`, `VALUE` read
/// as the run's `type` unless it names another, with its `decimals`, `width`
/// and `missing`. With `donor`, a named intermediate `D` over the `DONOR`
/// input, keyed by `KEY` = `SRC.ID`, serves the source. With `display`,
/// `VALUE` is saved too, written by the file's display precision with the
/// same decimals.
fn specification(run: &Json, display: bool) -> Json {
    let source = run["source"].as_str().unwrap_or("VALUE");
    let mut decimal = json!({"source": source, "decimals": run["decimals"].clone()});
    for field in ["width", "missing"] {
        if let Some(value) = run.get(field) {
            decimal[field] = value.clone();
        }
    }
    let output = if display {
        json!({
            "path": "output.csv",
            "columns": ["ID", "VALUE", "TEXT"],
            "decimals": run["decimals"].clone(),
        })
    } else {
        json!({"path": "output.csv", "columns": ["ID", "TEXT"]})
    };
    let mut specification = json!({
        "yamaa": "1.0",
        "type": "spec",
        "domain": "TEST",
        "keys": ["ID"],
        "input": {"SRC": {"path": "input.csv", "types": {"VALUE": run["type"].clone()}}},
        "output": output,
        "columns": [
            {"name": "ID", "type": "str", "derivation": "SRC.ID"},
            {"name": "VALUE", "type": run["type"].clone(), "derivation": "SRC.VALUE"},
            {"name": "TEXT", "type": "str", "derivation": {"str_decimal": decimal}}
        ]
    });
    if run.get("donor").is_some() {
        specification["base"] = json!("SRC");
        specification["input"]["DONOR"] = json!("donor.csv");
        specification["intermediates"] =
            json!([{"id": "D", "dataset": "DONOR", "key": {"KEY": "SRC.ID"}}]);
    }
    specification
}

/// A run's rows as the input writes them, and the bytes saved from them:
/// each authored text, after the value itself when `display` saves it.
fn files(run: &Json, display: bool) -> (String, String) {
    let mut input = String::from("ID,VALUE\n");
    let mut saved = String::from(if display {
        "ID,VALUE,TEXT\n"
    } else {
        "ID,TEXT\n"
    });
    for (index, row) in run["rows"].as_array().unwrap().iter().enumerate() {
        let id = format!("r{index:03}");
        let value = row["value"].as_str().unwrap_or("");
        let text = row["text"].as_str().unwrap_or("");
        if let Some(bits) = row["bits"].as_str() {
            assert_eq!(
                value.parse::<f64>().unwrap().to_bits(),
                u64::from_str_radix(bits, 16).unwrap(),
                "{}: the input writes the row's binary64",
                row["row"].as_str().unwrap()
            );
        }
        input.push_str(&format!("{id},{value}\n"));
        let shown = if display {
            format!("{text},")
        } else {
            String::new()
        };
        saved.push_str(&format!("{id},{shown}{text}\n"));
    }
    (input, saved)
}

/// The study of one run, its files written.
fn study(run: &Json, display: bool) -> Study {
    let study = Study::new("str-decimal");
    study.write(
        "spec.yaml",
        yamaa_adapters::json_wire::encode(&specification(run, display)),
    );
    study.write("input.csv", files(run, display).0);
    if let Some(donor) = run["donor"].as_str() {
        study.write("donor.csv", donor);
    }
    study
}

/// The saved bytes and the handler counts of one run that builds.
fn saved(run: &Json, display: bool) -> (String, Json) {
    let study = study(run, display);
    let label = run["run"].as_str().unwrap();
    let mut result = study
        .run("spec.yaml")
        .unwrap_or_else(|issues| panic!("{label}: {issues:?}"));
    assert!(result.issues().is_empty(), "{label}: {:?}", result.issues());
    let handlers = result.observations()["handler_counts"].clone();
    assert!(result.save().unwrap(), "{label}");
    (
        String::from_utf8(study.saved("output.csv")).unwrap(),
        handlers,
    )
}

/// Every run of the reference: a value is rounded by the one rounding and
/// written with exactly the declared decimals, then padded to the width, a
/// missing value takes the declared answer or stays missing, counted under
/// `missing` either way; a run with `issues` is refused with exactly those
/// and saves nothing.
#[test]
fn every_reference_run_saves_its_authored_text_or_reports_its_authored_issues() {
    for run in reference()["runs"].as_array().unwrap() {
        let label = run["run"].as_str().unwrap();
        if let Some(expected) = run.get("issues") {
            let study = study(run, false);
            assert_eq!(
                Json::Array(issues(study.run("spec.yaml"))),
                *expected,
                "{label}"
            );
            let written = std::path::Path::new(&study.path("output.csv")).exists();
            assert!(!written, "{label}: a refused run saves nothing");
            continue;
        }
        let missing = run["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["value"].is_null())
            .count();
        assert_eq!(
            saved(run, false),
            (
                files(run, false).1,
                json!([{
                    "spec_path": "columns.TEXT.derivation.str_decimal.missing",
                    "handler": "missing",
                    "count": missing,
                }])
            ),
            "{label}"
        );
    }
}

/// CSV display precision and `str_decimal` share one rounding and one
/// writer: a `float` column saved with `output.decimals` gives each tie and
/// near tie of the reference the text `str_decimal` gives it.
#[test]
fn display_precision_writes_each_reference_value_as_str_decimal_does() {
    let reference = reference();
    let runs = reference["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|run| run["display"].as_bool() == Some(true))
        .collect::<Vec<_>>();
    assert_eq!(runs.len(), 5);
    for run in runs {
        let label = run["run"].as_str().unwrap();
        assert_eq!(saved(run, true).0, files(run, true).1, "{label}");
    }
}
