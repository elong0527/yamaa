#![cfg(feature = "yaml")]
//! A named lookup is read only by rows that actually read it (REQ-1264).
//! Listing the column whose derivation reads the lookup in `keys` must not
//! make suppressed candidates read it: a candidate the row template's
//! `filter` discards never becomes a row, so an intermediate without
//! `no_match` must not fail `unmatched_key` for it (#2699). A kept row that
//! does read the lookup still fails when nothing matches.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
use yamaa_core::domain_entry::Request;

struct Fixture(PathBuf);
impl Fixture {
    fn new(specification: &str, files: &[(&str, &str)]) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "yamaa-key-lookup-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("spec.yaml"), specification).unwrap();
        for (name, body) in files {
            fs::write(root.join(name), body).unwrap();
        }
        Self(root)
    }
    fn run(&self) -> Result<file_application::Domain, Vec<Issue>> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        file_application::domain(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "key-lookup-reads",
                specification,
                base_directory: ".",
            },
        )
    }
    fn saved(&self, output: &str) -> String {
        let mut domain = self.run().unwrap();
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        assert!(domain.save().unwrap());
        fs::read_to_string(self.0.join(output)).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

// S2 has one blood pressure record and it has no result. LAST selects the
// latest record that has a result, so it has nothing for S2.
const VS: &str = "USUBJID,VSSEQ,VSTESTCD,VSDTC,VSSTRESN\n\
    S1,1,SYSBP,2025-01-01,120\n\
    S1,2,SYSBP,2025-01-02,124\n\
    S2,1,SYSBP,2025-01-01,\n";

// LAST: the latest record with a result for the subject and test; no no_match.
const HEAD: &str = "schema_version: \"1.0\"\n\
    domain: ADVS\n\
    input:\n\
    \x20 VS: {path: vs.csv, types: {VSSEQ: int, VSSTRESN: float}}\n\
    intermediates:\n\
    \x20 - id: LAST\n\
    \x20   dataset: VS\n\
    \x20   key: {USUBJID: USUBJID, VSTESTCD: PARAMCD}\n\
    \x20   filter: \"VS.VSSTRESN IS NOT NULL\"\n\
    \x20   order_by: [VS.VSDTC]\n\
    \x20   keep: last\n";

// One mean per subject and test; a group without a result is suppressed.
const GROUPED: &str = "keys: KEYS\n\
    output: {path: advs.csv, columns: [USUBJID, PARAMCD, SRCSEQ, AVAL]}\n\
    columns:\n\
    \x20 - {name: USUBJID, type: str}\n\
    \x20 - {name: PARAMCD, type: str}\n\
    \x20 - {name: SRCSEQ, type: int, derivation: LAST.VSSEQ}\n\
    \x20 - {name: AVAL, type: float}\n\
    rows:\n\
    \x20 - id: mean\n\
    \x20   dataset: VS\n\
    \x20   group_by: [VS.USUBJID, VS.VSTESTCD]\n\
    \x20   filter: \"AVAL IS NOT NULL\"\n\
    \x20   derivations:\n\
    \x20     USUBJID: VS.USUBJID\n\
    \x20     PARAMCD: VS.VSTESTCD\n\
    \x20     AVAL: {aggregate: {expr: \"MEAN(VS.VSSTRESN)\"}}\n";

// One row per record with a result; the record without one is suppressed.
const RECORDS: &str = "keys: KEYS\n\
    output: {path: advs.csv, columns: [USUBJID, VSSEQ, LASTSEQ, AVAL]}\n\
    columns:\n\
    \x20 - {name: USUBJID, type: str}\n\
    \x20 - {name: PARAMCD, type: str}\n\
    \x20 - {name: VSSEQ, type: int}\n\
    \x20 - {name: LASTSEQ, type: int, derivation: LAST.VSSEQ}\n\
    \x20 - {name: AVAL, type: float}\n\
    rows:\n\
    \x20 - id: results\n\
    \x20   dataset: VS\n\
    \x20   filter: \"VS.VSSTRESN IS NOT NULL\"\n\
    \x20   derivations:\n\
    \x20     USUBJID: VS.USUBJID\n\
    \x20     PARAMCD: VS.VSTESTCD\n\
    \x20     VSSEQ: VS.VSSEQ\n\
    \x20     AVAL: VS.VSSTRESN\n";

fn specification(body: &str, keys: &str) -> String {
    format!("{HEAD}{}", body.replace("KEYS", keys))
}

#[test]
fn grouped_key_column_is_not_read_for_a_suppressed_group() {
    let in_keys = Fixture::new(
        &specification(GROUPED, "[USUBJID, PARAMCD, SRCSEQ]"),
        &[("vs.csv", VS)],
    );
    let out_keys = Fixture::new(
        &specification(GROUPED, "[USUBJID, PARAMCD]"),
        &[("vs.csv", VS)],
    );
    // The suppressed S2 group never reads LAST, whatever `keys` declares,
    // and the built rows are the same either way.
    assert_eq!(in_keys.saved("advs.csv"), out_keys.saved("advs.csv"));
    assert_eq!(
        in_keys.saved("advs.csv"),
        "USUBJID,PARAMCD,SRCSEQ,AVAL\nS1,SYSBP,2,122\n"
    );
}

#[test]
fn ungrouped_key_column_is_not_read_for_a_suppressed_record() {
    let in_keys = Fixture::new(
        &specification(RECORDS, "[USUBJID, VSSEQ, LASTSEQ]"),
        &[("vs.csv", VS)],
    );
    let out_keys = Fixture::new(
        &specification(RECORDS, "[USUBJID, VSSEQ]"),
        &[("vs.csv", VS)],
    );
    assert_eq!(in_keys.saved("advs.csv"), out_keys.saved("advs.csv"));
    assert_eq!(
        in_keys.saved("advs.csv"),
        "USUBJID,VSSEQ,LASTSEQ,AVAL\nS1,1,2,120\nS1,2,2,124\n"
    );
}

// S2 now has a result of 90, so its row is kept; LAST only keeps records
// above 100, so the kept S2 row reads LAST and finds nothing.
const VS_KEPT: &str = "USUBJID,VSSEQ,VSTESTCD,VSDTC,VSSTRESN\n\
    S1,1,SYSBP,2025-01-01,120\n\
    S1,2,SYSBP,2025-01-02,124\n\
    S2,1,SYSBP,2025-01-01,90\n";

fn strict_head() -> String {
    HEAD.replace("VS.VSSTRESN IS NOT NULL", "VS.VSSTRESN > 100")
}

fn assert_unmatched_key(fixture: &Fixture) {
    let domain = fixture.run().unwrap();
    assert!(
        domain.issues().iter().any(|issue| {
            let value = issue.as_value();
            value["condition"] == "unmatched_key" && value["requirement"] == "REQ-0124"
        }),
        "{:?}",
        domain.issues()
    );
}

#[test]
fn grouped_unmatched_key_still_fails_for_a_kept_group() {
    let fixture = Fixture::new(
        &format!(
            "{}{}",
            strict_head(),
            GROUPED.replace("KEYS", "[USUBJID, PARAMCD, SRCSEQ]")
        ),
        &[("vs.csv", VS_KEPT)],
    );
    assert_unmatched_key(&fixture);
}

#[test]
fn ungrouped_unmatched_key_still_fails_for_a_kept_record() {
    let fixture = Fixture::new(
        &format!(
            "{}{}",
            strict_head(),
            RECORDS.replace("KEYS", "[USUBJID, VSSEQ, LASTSEQ]")
        ),
        &[("vs.csv", VS_KEPT)],
    );
    assert_unmatched_key(&fixture);
}
