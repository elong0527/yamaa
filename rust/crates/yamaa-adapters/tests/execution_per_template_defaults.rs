#![cfg(feature = "yaml")]
//! Row-default promotion is scoped per template (#2754).
//! A selector read in template A promotes the key column whose derivation
//! reads a named lookup for A only. A suppressed candidate in template B
//! must not read that lookup before its filter, so an intermediate without
//! `absent` must not fail `unmatched_key` for it (LOOKUP-12), while A
//! still completes its keys.
use std::fs;
use yamaa_adapters::file_application;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::domain_entry::Request;

struct Fixture(crate::support::Scratch);
impl Fixture {
    fn new(specification: &str, files: &[(&str, &str)]) -> Self {
        let root = crate::support::Scratch::new("per-template");
        fs::write(root.join("spec.yaml"), specification).unwrap();
        for (name, body) in files {
            fs::write(root.join(name), body).unwrap();
        }
        Self(root)
    }
    fn run(&self) -> Result<file_application::Domain, Vec<Diagnostic>> {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        file_application::domain(
            yamaa_core::project_function::Language::Python,
            Request {
                specification,
                environment: None,
            },
            crate::support::study::identity("per-template-defaults", specification),
        )
    }
    fn saved(&self, output: &str) -> String {
        let mut domain = self.run().unwrap();
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        assert!(domain.save().unwrap());
        fs::read_to_string(self.0.join(output)).unwrap()
    }
}

// S2 has one blood pressure record and it has no result. LAST selects the
// latest record that has a result, so it has nothing for S2. DM carries S1
// only, so template A never sees the S2 candidate.
const VS: &str = "USUBJID,VSSEQ,VSTESTCD,VSDTC,VSSTRESN\n\
    S1,1,SYSBP,2025-01-01,120\n\
    S1,2,SYSBP,2025-01-02,124\n\
    S2,1,SYSBP,2025-01-01,\n";
const DM: &str = "USUBJID\nS1\n";

// LAST: the latest record with a result for the subject and test; no absent.
const HEAD: &str = "yamaa: \"1.0\"\n\
    type: spec\n\
    domain: ADVS\n\
    input:\n\
    \x20 VS: {path: vs.csv, types: {VSSEQ: int, VSSTRESN: float}}\n\
    \x20 DM: dm.csv\n\
    intermediates:\n\
    \x20 - id: LAST\n\
    \x20   dataset: VS\n\
    \x20   key: {USUBJID: USUBJID, VSTESTCD: PARAMCD}\n\
    \x20   filter: \"VS.VSSTRESN IS NOT NULL\"\n\
    \x20   order_by: [VS.VSDTC]\n\
    \x20   keep: last\n";

// Template A reads LAST, so it needs complete keys before its rows. Template
// B inherits the SRCSEQ default but never reads LAST in row phase; its S2
// candidate is suppressed by its filter and must never read LAST.
const GROUPED: &str = "keys: [USUBJID, PARAMCD, SRCSEQ, ROWSRC]\n\
    output: {path: advs.csv, columns: [USUBJID, PARAMCD, SRCSEQ, AVAL, ROWSRC]}\n\
    columns:\n\
    \x20 - {name: USUBJID, type: str}\n\
    \x20 - {name: PARAMCD, type: str}\n\
    \x20 - {name: SRCSEQ, type: int, derivation: LAST.VSSEQ}\n\
    \x20 - {name: AVAL, type: float}\n\
    \x20 - {name: ROWSRC, type: str}\n\
    rows:\n\
    \x20 - id: keeper\n\
    \x20   dataset: DM\n\
    \x20   derivations:\n\
    \x20     USUBJID: DM.USUBJID\n\
    \x20     PARAMCD: \"'SYSBP'\"\n\
    \x20     SRCSEQ: LAST.VSSEQ\n\
    \x20     AVAL: LAST.VSSTRESN\n\
    \x20     ROWSRC: \"'A'\"\n\
    \x20 - id: mean\n\
    \x20   dataset: VS\n\
    \x20   group_by: [VS.USUBJID, VS.VSTESTCD]\n\
    \x20   filter: \"AVAL IS NOT NULL\"\n\
    \x20   derivations:\n\
    \x20     USUBJID: VS.USUBJID\n\
    \x20     PARAMCD: VS.VSTESTCD\n\
    \x20     AVAL: {aggregate: \"MEAN(VS.VSSTRESN)\"}\n\
    \x20     ROWSRC: \"'B'\"\n";

const UNGROUPED: &str = "keys: [USUBJID, VSSEQ, SRCSEQ, ROWSRC]\n\
    output: {path: advs.csv, columns: [USUBJID, VSSEQ, SRCSEQ, AVAL, ROWSRC]}\n\
    columns:\n\
    \x20 - {name: USUBJID, type: str}\n\
    \x20 - {name: PARAMCD, type: str}\n\
    \x20 - {name: VSSEQ, type: int}\n\
    \x20 - {name: SRCSEQ, type: int, derivation: LAST.VSSEQ}\n\
    \x20 - {name: AVAL, type: float}\n\
    \x20 - {name: ROWSRC, type: str}\n\
    rows:\n\
    \x20 - id: keeper\n\
    \x20   dataset: DM\n\
    \x20   derivations:\n\
    \x20     USUBJID: DM.USUBJID\n\
    \x20     PARAMCD: \"'SYSBP'\"\n\
    \x20     VSSEQ: LAST.VSSEQ\n\
    \x20     SRCSEQ: LAST.VSSEQ\n\
    \x20     AVAL: LAST.VSSTRESN\n\
    \x20     ROWSRC: \"'A'\"\n\
    \x20 - id: results\n\
    \x20   dataset: VS\n\
    \x20   filter: \"VS.VSSTRESN IS NOT NULL\"\n\
    \x20   derivations:\n\
    \x20     USUBJID: VS.USUBJID\n\
    \x20     PARAMCD: VS.VSTESTCD\n\
    \x20     VSSEQ: VS.VSSEQ\n\
    \x20     AVAL: VS.VSSTRESN\n\
    \x20     ROWSRC: \"'B'\"\n";

#[test]
fn grouped_suppressed_candidate_does_not_read_lookup() {
    let fixture = Fixture::new(
        &format!("{HEAD}{GROUPED}"),
        &[("vs.csv", VS), ("dm.csv", DM)],
    );
    // Template A completes its keys from LAST; the suppressed S2 group in
    // template B never reads LAST, so the build succeeds.
    assert_eq!(
        fixture.saved("advs.csv"),
        "USUBJID,PARAMCD,SRCSEQ,AVAL,ROWSRC\nS1,SYSBP,2,124,A\nS1,SYSBP,2,122,B\n"
    );
}

#[test]
fn ungrouped_suppressed_candidate_does_not_read_lookup() {
    let fixture = Fixture::new(
        &format!("{HEAD}{UNGROUPED}"),
        &[("vs.csv", VS), ("dm.csv", DM)],
    );
    assert_eq!(
        fixture.saved("advs.csv"),
        "USUBJID,VSSEQ,SRCSEQ,AVAL,ROWSRC\nS1,2,2,124,A\nS1,1,2,120,B\nS1,2,2,124,B\n"
    );
}
