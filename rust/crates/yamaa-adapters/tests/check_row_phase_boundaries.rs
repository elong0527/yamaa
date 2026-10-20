#![cfg(feature = "yaml")]
//! `check` reports a row template's phase boundaries as `domain` does
//! (#2754): a row derivation that reads a column its template leaves to the
//! column phase (COLUMN-4), directly or through a lookup that matches on it
//! (LOOKUP-2). Every case lists the same issues in both routes and hosts.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::{diagnostic::Diagnostic, domain_entry::Request, project_function::Language};

struct Fixture(PathBuf);
impl Fixture {
    fn new(specification: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "yamaa-row-phase-boundaries-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("spec.yaml"), specification).unwrap();
        fs::write(
            root.join("vs.csv"),
            "STUDYID,USUBJID,VSDTC,SYSBP,DIABP\nS,S-1,2025-01-01,120,80\n",
        )
        .unwrap();
        fs::write(
            root.join("plan.csv"),
            "STUDYID,DOSENUM,PLANDOSE,FIRST,LAST,REF\nS,1,10,1,2,1\n",
        )
        .unwrap();
        Self(root)
    }
    /// The issues of `check` and of `domain`, as each host reports them.
    fn issues(&self, host: Language) -> (Vec<String>, Vec<String>) {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        let request = || Request {
            specification,
            environment: None,
        };
        let identity = || Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "row-phase-boundaries",
            specification,
            base_directory: ".",
        };
        let render = |issues: &[Diagnostic]| {
            issues
                .iter()
                .map(|issue| {
                    format!(
                        "{} {:?} {:?} {:?}",
                        issue.condition(),
                        issue.requirement(),
                        issue.spec_paths,
                        issue.context
                    )
                })
                .collect::<Vec<_>>()
        };
        let checked = render(&file_application::check(host, request(), identity()));
        let built = match file_application::domain(host, request(), identity()) {
            Ok(domain) => render(&domain.issues()),
            Err(issues) => render(&issues),
        };
        (checked, built)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

/// Two templates over VS, each deriving `derivation` (one YAML line under
/// `derivations`), with the lookups `intermediates` declares. AVISITN is a
/// window, which only the column phase completes.
fn specification(intermediates: &str, derivation: &str) -> String {
    let template = |id: &str, field: &str| {
        format!(
            "  - id: {id}\n    dataset: VS\n    derivations:\n      PARAMCD: {{literal: {field}}}\n      AVAL: VS.{field}\n      {derivation}\n"
        )
    };
    format!(
        "schema_version: \"1.0\"\n\
         domain: ADVS\n\
         keys: [STUDYID, USUBJID, PARAMCD, AVISITN]\n\
         input:\n\
         \x20 VS: {{path: vs.csv, types: {{VSDTC: date, SYSBP: float, DIABP: float}}}}\n\
         \x20 PLAN: {{path: plan.csv, types: {{DOSENUM: int, PLANDOSE: float, FIRST: int, LAST: int, REF: int}}}}\n\
         {intermediates}\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, PARAMCD, AVISITN, AVAL, X, C]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str, derivation: VS.STUDYID}}\n\
         \x20 - {{name: USUBJID, type: str, derivation: VS.USUBJID}}\n\
         \x20 - {{name: PARAMCD, type: str}}\n\
         \x20 - {{name: ADT, type: date, derivation: VS.VSDTC}}\n\
         \x20 - name: AVISITN\n\
         \x20   type: int\n\
         \x20   derivation: {{rank: {{method: dense, window: {{group_by: [STUDYID, USUBJID], order_by: [ADT]}}}}}}\n\
         \x20 - {{name: AVAL, type: float}}\n\
         \x20 - {{name: X, type: float, derivation: {{literal: 0.0}}}}\n\
         \x20 - {{name: C, type: str, derivation: {{literal: none}}}}\n\
         rows:\n{}{}",
        template("systolic", "SYSBP"),
        template("diastolic", "DIABP"),
    )
}

const PLANNED: &str =
    "intermediates:\n  - {id: PLANNED, dataset: PLAN, key: {DOSENUM: AVISITN}, no_match: null}\n";
const PERIOD: &str = "intermediates:\n  - {id: PERIOD, dataset: PLAN, key: [STUDYID], between: {value: AVISITN, lower: FIRST, upper: LAST}, no_match: null}\n";
const CHAIN: &str = "intermediates:\n  - {id: FIRSTREF, dataset: PLAN, key: {DOSENUM: AVISITN}, no_match: null}\n  - {id: PERIOD, dataset: PLAN, key: [STUDYID], between: {value: FIRSTREF.REF, lower: FIRST, upper: LAST}, no_match: null}\n";

#[test]
fn check_reports_each_row_phase_boundary_as_domain_does() {
    let cases = [
        ("", "C: {flag: \"AVISITN = 1\"}"),
        ("", "X: {compute: {expr: \"AVISITN + AVAL * AVISITN\"}}"),
        (
            "",
            "X: {case: [{when: \"AVISITN = 1\", then: {literal: 1.0}}, {otherwise: AVISITN}]}",
        ),
        ("", "X: {round_half_away_from_zero: {source: AVISITN, digits: 0}}"),
        ("", "X: {greatest: {sources: [AVISITN, AVAL]}}"),
        ("", "X: {first_available: {sources: [AVISITN, AVAL]}}"),
        ("", "C: {str_template: \"{AVISITN}\"}"),
        // The phase boundary stops binding before the mapping's source type.
        ("", "C: {mapping: {source: AVISITN, dict: {\"1\": one}}}"),
        // A structured source's own range value.
        (
            "",
            "X: {source: {variable: PLAN.PLANDOSE, between: {value: AVISITN, lower: FIRST, upper: LAST}}}",
        ),
        ("", "X: {row_number: {window: {order_by: [AVISITN]}}}"),
        (PLANNED, "X: PLANNED.PLANDOSE"),
        (PLANNED, "X: {compute: {expr: \"PLANNED.PLANDOSE + AVISITN\"}}"),
        (PERIOD, "X: PERIOD.PLANDOSE"),
        // Each range value followed to the key value that reads AVISITN.
        (CHAIN, "X: PERIOD.PLANDOSE"),
    ];
    for (intermediates, derivation) in cases {
        let fixture = Fixture::new(&specification(intermediates, derivation));
        for host in [Language::Python, Language::R] {
            let (checked, built) = fixture.issues(host);
            assert_eq!(checked, built, "{derivation} ({host:?})");
            assert!(
                checked
                    .first()
                    .is_some_and(|issue| issue.starts_with("phase_boundary")),
                "{derivation} ({host:?}): {checked:?}"
            );
        }
    }
}
