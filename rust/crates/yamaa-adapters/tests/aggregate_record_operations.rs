//! Independent YAML, donor bytes and exact saved truth for per-record aggregates.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-aggregate-records-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(&self) -> Result<file_application::Domain, Vec<yamaa_adapters::issue_rows::Issue>> {
        let specification = self.0.join("spec.yaml").to_str().unwrap().to_owned();
        file_application::domain(
            Request {
                specification: &specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "test",
                specification: &specification,
                base_directory: ".",
            },
        )
    }
    fn save(&self) -> Vec<u8> {
        let mut domain = self.run().unwrap_or_else(|issues| panic!("{issues:?}"));
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        assert!(domain.save().unwrap());
        fs::read(self.0.join("result.csv")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn specification(operation: &str) -> String {
    format!("schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput:\n  A: {{path: a.csv, types: {{CUT: int}}}}\n  B: {{path: b.csv, types: {{N: int}}}}\nbase: A\noutput: {{path: result.csv, columns: [ID, CUT, TOTAL]}}\ncolumns:\n  - {{name: ID, type: str, derivation: A.ID}}\n  - {{name: CUT, type: int, derivation: A.CUT}}\n  - name: TOTAL\n    type: float\n    derivation:\n      aggregate:\n{operation}")
}
#[test]
fn ordered_converted_bindings_reduce_only_matched_filtered_inclusive_records() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("a.csv"), b"ID,CUT\na,3\nb,2\nc,\nd,1\n").unwrap();
    fs::write(fixture.0.join("b.csv"),b"ID,N,V,USE\na,2,1.5,Y\na,3,2,Y\na,4,invalid,Y\na,1,invalid,N\nb,2,bad,Y\nc,1,99,Y\nd,2,99,Y\n").unwrap();
    fs::write(fixture.0.join("spec.yaml"),specification("        filter: B.USE = 'Y'\n        between: {value: CUT, lower: N}\n        derive:\n          - name: VALUE\n            type: float\n            derivation: {value: {source: B.V}, unconvertible: 0}\n          - name: TWICE\n            type: float\n            derivation: {compute: {expr: 'VALUE * 2'}}\n        expr: SUM(TWICE)\n")).unwrap();
    assert_eq!(fixture.save(), b"ID,CUT,TOTAL\na,3,7\nb,2,0\nc,,\nd,1,\n");
}
#[test]
fn original_public_aggregate_fixtures_preserve_frozen_csv_bytes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
    fn copy(from: &Path, to: &Path) {
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() == "expected" {
                continue;
            }
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                fs::create_dir(&target).unwrap();
                copy(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    for (name, result) in [
        ("schema-aggregation", "adsl.csv"),
        ("schema-derive-date-epoch", "adex.csv"),
        ("schema-derive-text-score", "adqs.csv"),
        ("schema-aggregate-derive-intermediate", "adex.csv"),
        ("adam-adsl-new-therapy", "adsl.csv"),
        ("adam-adtr-nadir", "adtr.csv"),
        ("adam-adtte-pfs", "adtte.csv"),
        ("sdtm-rs-timepoint-response", "rs.csv"),
    ] {
        let fixture = Fixture::new();
        copy(&root.join(name), &fixture.0);
        let mut domain = fixture
            .run()
            .unwrap_or_else(|issues| panic!("{name}: {issues:?}"));
        assert!(domain.issues().is_empty(), "{name}: {:?}", domain.issues());
        assert!(domain.save().unwrap(), "{name}");
        assert_eq!(
            fs::read(fixture.0.join(result)).unwrap(),
            fs::read(root.join(name).join("expected").join(result)).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn derive_and_range_faults_fail_before_any_output_with_registered_causes() {
    for (operation,condition,requirement) in [
        ("        derive: []\n        expr: SUM(V)\n","invalid_derive_step","REQ-1189"),
        ("        derive:\n          - {name: V, type: float, derivation: W}\n        expr: SUM(V)\n","unknown_derive_variable","REQ-1189"),
        ("        derive:\n          - {name: V, type: float, derivation: {literal: 1}}\n        expr: SUM(V)\n","invalid_derive_step","REQ-1191"),
        ("        between: {value: CUT, lower: USE}\n        expr: SUM(B.N)\n","incomparable_range_types","REQ-0121"),
        ("        between: {value: CUT, lower: ABSENT}\n        expr: SUM(B.N)\n","unknown_field","REQ-0121"),
    ] {
        let fixture=Fixture::new();
        fs::write(fixture.0.join("a.csv"),b"ID,CUT\na,2\n").unwrap();
        fs::write(fixture.0.join("b.csv"),b"ID,N,USE\na,2,Y\n").unwrap();
        fs::write(fixture.0.join("spec.yaml"),specification(operation)).unwrap();
        let issues=match fixture.run(){Ok(mut domain)=>{assert!(domain.output().is_none());assert!(domain.save().is_err());domain.issues()},Err(issues)=>issues};
        assert!(issues.iter().any(|issue|issue.condition==condition && issue.requirement.as_deref()==Some(requirement)),"{issues:?}");
        assert!(!fixture.0.join("result.csv").exists());
    }
}
#[test]
fn named_derive_read_requires_keep_and_uses_original_current_record() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("a.csv"), b"ID,CUT\na,2\nb,3\n").unwrap();
    fs::write(fixture.0.join("b.csv"), b"ID,N\nb,10\na,1\na,4\n").unwrap();
    let operation="        derive:\n          - {name: N, type: int, derivation: B.N}\n          - {name: C, type: int, derivation: CAP.CUT}\n          - {name: VALUE, type: int, derivation: {least: {sources: [N, C]}}}\n        expr: SUM(VALUE)\n";
    let spec=specification(operation).replace("base: A\n","base: A\nintermediates:\n  - {id: CAP, dataset: A, key: {ID: A.ID}, order_by: [A.CUT], keep: first}\n");
    fs::write(fixture.0.join("spec.yaml"), &spec).unwrap();
    assert_eq!(fixture.save(), b"ID,CUT,TOTAL\na,2,3\nb,3,3\n");
    fs::write(
        fixture.0.join("spec.yaml"),
        spec.replace(", order_by: [A.CUT], keep: first", ""),
    )
    .unwrap();
    let issues = match fixture.run() {
        Ok(domain) => domain.issues(),
        Err(issues) => issues,
    };
    assert!(
        issues
            .iter()
            .any(|issue| issue.condition == "invalid_derive_step"
                && issue.requirement.as_deref() == Some("REQ-1242")),
        "{issues:?}"
    );
}
