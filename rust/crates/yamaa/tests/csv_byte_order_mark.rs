//! A csv input declares a leading UTF-8 byte-order mark (CSV-10, #3030 D15
//! and D79): with `byte_order_mark: true` the source must begin with one
//! mark, which is skipped; without it a mark fails as in 0.5.0. Both faults
//! are `source_byte_order_mark`, whose context records the declaration, and
//! the declaration on an input that another profile reads fails before any
//! input is read.
#![cfg(feature = "yaml")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use arrow_array::{Array, ListArray, StringArray};
use yamaa::{CancellationToken, ContextValue, Engine, EngineOptions, HostLanguage, Table};
use yamaa_core::json::Value;

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Study(PathBuf);
impl Study {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "yamaa-csv-byte-order-mark-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(yamaa_adapters::canonical_path::canonicalize(root).unwrap())
    }
    fn write(&self, name: &str, contents: impl AsRef<[u8]>) {
        std::fs::write(self.0.join(name), contents).unwrap();
    }
    fn read(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.0.join(name)).unwrap()
    }
    fn path(&self, name: &str) -> String {
        let path = self.0.join(name).to_str().unwrap().to_owned();
        #[cfg(windows)]
        let path = path.replace('\\', "/");
        path
    }
}
impl Drop for Study {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn engine() -> Engine {
    Engine::new(EngineOptions::new(
        HostLanguage::Python,
        "csv-byte-order-mark-test".into(),
        "0".into(),
    ))
}

/// One issue row: condition, rule, spec paths, message and context.
#[derive(Debug)]
struct Issue {
    condition: String,
    rule: Option<String>,
    paths: Vec<String>,
    message: String,
    context: Value,
}

fn issues(table: &Table) -> Vec<Issue> {
    let mut issues = Vec::new();
    for batch in table.batches() {
        let text = |name: &str| {
            batch
                .column_by_name(name)
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .clone()
        };
        let (condition, rule, message, context) = (
            text("condition"),
            text("rule"),
            text("message"),
            text("context"),
        );
        let paths = batch
            .column_by_name("spec_paths")
            .unwrap()
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap()
            .clone();
        for row in 0..batch.num_rows() {
            let values = paths.value(row);
            let values = values.as_any().downcast_ref::<StringArray>().unwrap();
            issues.push(Issue {
                condition: condition.value(row).to_owned(),
                rule: (!rule.is_null(row)).then(|| rule.value(row).to_owned()),
                paths: (0..values.len())
                    .map(|i| values.value(i).to_owned())
                    .collect(),
                message: message.value(row).to_owned(),
                context: yamaa_adapters::json_wire::decode(context.value(row)).unwrap(),
            });
        }
    }
    issues
}

/// A 1.0 specification reading `RAW` as `input` declares it.
fn spec(input: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: DM\nkeys: [USUBJID]\ninput:\n  RAW: {input}\ncolumns:\n  - {{name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}}\n  - {{name: SITE, type: str, label: Site, derivation: RAW.SITE}}\noutput: {{path: dm.csv, columns: [USUBJID, SITE]}}\n"
    )
}

/// A spreadsheet's UTF-8 export: the mark, then records ending in CR LF.
const MARKED: &[u8] = b"\xef\xbb\xbfUSUBJID,SITE\r\nP1,H\xc3\xb4pital\r\nP2,Clinic\r\n";
/// The listing every run writes: UTF-8 without a mark, records ending in LF
/// (CSV-1).
const LISTING: &[u8] = b"USUBJID,SITE\nP1,H\xc3\xb4pital\nP2,Clinic\n";

/// The value at `key` of a resolved tree's map.
fn field<'a>(value: &'a ContextValue, key: &str) -> &'a ContextValue {
    let ContextValue::Map(map) = value else {
        panic!("a map: {value:?}")
    };
    &map[key]
}

#[test]
fn a_declared_mark_is_skipped_by_every_specification_call() {
    let study = Study::new();
    study.write("raw.csv", MARKED);
    study.write("spec.yaml", spec("{path: raw.csv, byte_order_mark: true}"));
    let spec = study.path("spec.yaml");
    let engine = engine();
    let token = CancellationToken::new();
    let checked = issues(&engine.spec_check(&spec, None, &token).unwrap().issues());
    assert!(checked.is_empty(), "{checked:?}");
    let mut run = engine.spec_run(&spec, None, &token).unwrap();
    let ran = issues(&run.issues());
    assert!(ran.is_empty(), "{ran:?}");
    run.save().unwrap();
    assert_eq!(study.read("dm.csv"), LISTING);
    // The resolved specification keeps the declaration as written.
    let resolved = engine.spec_resolve(&spec, None).unwrap();
    let ContextValue::List(entries) = field(&resolved, "entries") else {
        panic!("entries")
    };
    let input = field(field(field(&entries[0], "specification"), "input"), "RAW");
    assert_eq!(field(input, "byte_order_mark"), &ContextValue::Bool(true));
}

/// The one issue of a run of `spec` over `raw`.
fn run_issue(raw: &[u8], spec: &str) -> Issue {
    let study = Study::new();
    study.write("raw.csv", raw);
    study.write("spec.yaml", spec);
    let run = engine()
        .spec_run(&study.path("spec.yaml"), None, &CancellationToken::new())
        .unwrap();
    let mut found = issues(&run.issues());
    assert_eq!(found.len(), 1, "{found:?}");
    found.remove(0)
}

/// `source_byte_order_mark` at the header's first field of `raw.csv`, with
/// the input's declaration.
fn assert_mark(issue: &Issue, declared: bool) {
    assert_eq!(issue.condition, "source_byte_order_mark");
    assert_eq!(issue.rule.as_deref(), Some("CSV-16"));
    assert_eq!(issue.paths, ["input.RAW.path"]);
    assert_eq!(
        issue.context,
        yamaa_adapters::json_wire::decode(&format!(
            "{{\"dataset\":\"RAW\",\"declared\":{declared},\"field\":1,\"path\":\"raw.csv\",\"record\":1}}"
        ))
        .unwrap()
    );
    assert_eq!(
        issue.message,
        "raw.csv starts with a byte order mark that its input does not declare, or does not \
         start with the one it declares"
    );
}

#[test]
fn a_mark_the_input_does_not_declare_fails_on_both_surfaces() {
    assert_mark(&run_issue(MARKED, &spec("raw.csv")), false);
    // `false` declares no mark, as an omitted field does.
    assert_mark(
        &run_issue(MARKED, &spec("{path: raw.csv, byte_order_mark: false}")),
        false,
    );
    // A 0.5 file declares no mark and keeps the 0.5.0 refusal.
    let legacy = spec("raw.csv").replace("yamaa: \"1.0\"\ntype: spec\n", "schema_version: '1.0'\n");
    assert_mark(&run_issue(MARKED, &legacy), false);
}

#[test]
fn a_declared_mark_must_begin_the_source_exactly_once() {
    let declared = spec("{path: raw.csv, byte_order_mark: true}");
    assert_mark(&run_issue(LISTING, &declared), true);
    // Only the declared mark is skipped; a second one is not declared.
    let twice = [&b"\xef\xbb\xbf"[..], MARKED].concat();
    assert_mark(&run_issue(&twice, &declared), true);
}

#[test]
fn a_declared_mark_on_another_profile_fails_before_any_input_is_read() {
    let study = Study::new();
    // No input file exists: the declaration fails first.
    study.write(
        "spec.yaml",
        spec("{path: raw.parquet, byte_order_mark: true}"),
    );
    let path = study.path("spec.yaml");
    let engine = engine();
    let token = CancellationToken::new();
    for found in [
        issues(&engine.spec_check(&path, None, &token).unwrap().issues()),
        issues(&engine.spec_run(&path, None, &token).unwrap().issues()),
    ] {
        let [issue] = found.as_slice() else {
            panic!("one issue: {found:?}")
        };
        assert_eq!(issue.condition, "value_not_permitted");
        assert_eq!(issue.rule.as_deref(), Some("CSV-10"));
        assert_eq!(issue.paths, ["input.RAW.byte_order_mark"]);
        assert_eq!(
            issue.context,
            yamaa_adapters::json_wire::decode("{\"dataset\":\"RAW\"}").unwrap()
        );
        assert_eq!(
            issue.message,
            "input.RAW.byte_order_mark has a value that is not permitted"
        );
    }
}

#[test]
fn a_0_5_file_cannot_declare_a_mark() {
    let study = Study::new();
    study.write("raw.csv", MARKED);
    study.write(
        "spec.yaml",
        spec("{path: raw.csv, byte_order_mark: true}")
            .replace("yamaa: \"1.0\"\ntype: spec\n", "schema_version: '1.0'\n"),
    );
    let found = issues(
        &engine()
            .spec_check(&study.path("spec.yaml"), None, &CancellationToken::new())
            .unwrap()
            .issues(),
    );
    let [issue] = found.as_slice() else {
        panic!("one issue: {found:?}")
    };
    assert_eq!(issue.condition, "unknown_field");
    assert_eq!(issue.paths, ["input.RAW.byte_order_mark"]);
}
