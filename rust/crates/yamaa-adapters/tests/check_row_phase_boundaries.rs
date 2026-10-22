#![cfg(feature = "yaml")]
//! `check` reports a row template's phase boundaries as `domain` does
//! (#2754): a row derivation that reads a column its template leaves to the
//! column phase (COLUMN-4), directly or through a lookup that matches on it
//! (LOOKUP-2), and a template that names a column whose default is not row
//! local (SPEC-6). Every case lists the same issues in both routes and hosts.
use std::fs;
use yamaa_adapters::file_application;
use yamaa_core::{diagnostic::Diagnostic, domain_entry::Request, project_function::Language};

struct Fixture(crate::support::Scratch);
impl Fixture {
    fn new(specification: &str) -> Self {
        let root = crate::support::Scratch::new("row-phase-boundaries");
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
    fn diagnostics(&self, host: Language) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        let request = || Request {
            specification,
            environment: None,
        };
        let identity = || crate::support::study::identity("row-phase-boundaries", specification);
        let checked = file_application::check(host, request(), identity());
        let built = match file_application::domain(host, request(), identity()) {
            Ok(domain) => domain.issues(),
            Err(issues) => issues,
        };
        (checked, built)
    }
    /// The issues of `check` and of `domain`, each rendered on one line.
    fn issues(&self, host: Language) -> (Vec<String>, Vec<String>) {
        let render = |issues: Vec<Diagnostic>| {
            issues
                .iter()
                .map(|issue| {
                    format!(
                        "{} {:?} {:?} {:?}",
                        issue.condition(),
                        issue.rule(),
                        issue.spec_paths,
                        issue.context
                    )
                })
                .collect::<Vec<_>>()
        };
        let (checked, built) = self.diagnostics(host);
        (render(checked), render(built))
    }
}

/// Two templates over VS, each deriving `derivation` (one YAML line under
/// `derivations`), with the lookups `intermediates` declares. AVISITN is a
/// window, which only the column phase completes. PLAN declares its key, the
/// output key it stores, for the reads that join it (D23).
fn specification(intermediates: &str, derivation: &str) -> String {
    let template = |id: &str, field: &str| {
        format!(
            "  - id: {id}\n    dataset: VS\n    derivations:\n      PARAMCD: \"'{field}'\"\n      AVAL: VS.{field}\n      {derivation}\n"
        )
    };
    format!(
        "yamaa: \"1.0\"\ntype: spec\n\
         domain: ADVS\n\
         keys: [STUDYID, USUBJID, PARAMCD, AVISITN]\n\
         input:\n\
         \x20 VS: {{path: vs.csv, types: {{VSDTC: date, SYSBP: float, DIABP: float}}}}\n\
         \x20 PLAN: {{path: plan.csv, key: [STUDYID], types: {{DOSENUM: int, PLANDOSE: float, FIRST: int, LAST: int, REF: int}}}}\n\
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
         \x20 - {{name: X, type: float, derivation: 0.0}}\n\
         \x20 - {{name: C, type: str, derivation: \"'none'\"}}\n\
         rows:\n{}{}",
        template("systolic", "SYSBP"),
        template("diastolic", "DIABP"),
    )
}

const PLANNED: &str = "intermediates:\n  - {id: PLANNED, dataset: PLAN, key: {DOSENUM: AVISITN}}\n";
const PERIOD: &str = "intermediates:\n  - {id: PERIOD, dataset: PLAN, key: [STUDYID], between: {value: AVISITN, lower: FIRST, upper: LAST}}\n";
const CHAIN: &str = "intermediates:\n  - {id: FIRSTREF, dataset: PLAN, key: {DOSENUM: AVISITN}}\n  - {id: PERIOD, dataset: PLAN, key: [STUDYID], between: {value: FIRSTREF.REF, lower: FIRST, upper: LAST}}\n";

#[test]
fn check_reports_each_row_phase_boundary_as_domain_does() {
    let cases = [
        ("", "C: {flag: \"AVISITN = 1\"}"),
        ("", "X: \"AVISITN + AVAL * AVISITN\""),
        (
            "",
            "X: {case: [{when: \"AVISITN = 1\", then: 1.0}, {otherwise: AVISITN}]}",
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
        (PLANNED, "X: \"PLANNED.PLANDOSE + AVISITN\""),
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

/// The two spellings of one specification.
#[derive(Clone, Copy, Debug)]
enum Spelling {
    V0_5,
    V1_0,
}

/// A dosing specification in `spelling` whose one row template, `dosed`,
/// reads EX with the further `fields` and derives `derivations`. TOTAL's
/// default holds an aggregate only in a `case` result, and FIRSTSEQ's reads
/// the SELF intermediate FIRST.
fn dosing(spelling: Spelling, fields: &str, derivations: &str) -> String {
    // 1.0 declares the key that EX's keyless aggregate matches on (D63).
    let (header, key, zero) = match spelling {
        Spelling::V0_5 => ("schema_version: \"1.0\"\n", "", "{literal: 0.0}"),
        Spelling::V1_0 => (
            "yamaa: \"1.0\"\ntype: spec\n",
            ", key: [STUDYID, USUBJID]",
            "0.0",
        ),
    };
    format!(
        "{header}domain: ADEX\n\
         keys: [STUDYID, USUBJID, ASEQ]\n\
         input:\n\
         \x20 EX: {{path: ex.csv, types: {{EXSEQ: int, EXDOSE: float}}{key}}}\n\
         intermediates:\n\
         \x20 - {{id: FIRST, dataset: SELF, key: [STUDYID, USUBJID], order_by: [SELF.ASEQ], keep: first}}\n\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, ASEQ, TOTAL, FIRSTSEQ, SHARE]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str, derivation: EX.STUDYID}}\n\
         \x20 - {{name: USUBJID, type: str, derivation: EX.USUBJID}}\n\
         \x20 - {{name: ASEQ, type: int, derivation: EX.EXSEQ}}\n\
         \x20 - name: TOTAL\n\
         \x20   type: float\n\
         \x20   derivation: {{case: [{{when: \"ASEQ > 0\", then: {{aggregate: \"SUM(EX.EXDOSE)\"}}}}, {{otherwise: {zero}}}]}}\n\
         \x20 - {{name: FIRSTSEQ, type: int, derivation: FIRST.ASEQ}}\n\
         \x20 - {{name: SHARE, type: float}}\n\
         rows:\n\
         \x20 - id: dosed\n\
         \x20   dataset: EX\n\
         {fields}\
         \x20   derivations: {{{derivations}}}\n"
    )
}

#[test]
fn an_aggregate_in_a_case_result_is_not_row_local_in_either_spelling() {
    use Spelling::{V0_5, V1_0};
    // An aggregate makes its default not row local wherever it is written,
    // the whole derivation or a `case` result, as a window does (SPEC-6,
    // #3030 D132), and a template that reads the default no longer
    // completes it in its own scope.
    let duplicate = |column: &str, why: &str| {
        yamaa_adapters::json!({
            "phase": "validation",
            "condition": "duplicate_derivation",
            "rule": "SPEC-6",
            "spec_paths": [format!("columns.{column}.derivation")],
            "context": {"column": column, "rows": ["dosed"], "not_row_local": why}
        })
    };
    let boundary = |path: &str| {
        yamaa_adapters::json!({
            "phase": "validation",
            "condition": "phase_boundary",
            "rule": "COLUMN-4",
            "spec_paths": [path],
            "context": {
                "identifier": "TOTAL",
                "row": "dosed",
                "available_phase": "column_derivation",
                "required_phase": "row_construction"
            }
        })
    };
    let grouped = "\x20   group_by: [EX.STUDYID, EX.USUBJID, EX.EXSEQ]\n";
    for (spelling, fields, derivations, expected) in [
        // Naming TOTAL no longer replaces its default.
        (
            V0_5,
            "",
            "SHARE: {literal: 1.0}, TOTAL: {literal: 7.0}",
            duplicate("TOTAL", "aggregate"),
        ),
        (
            V1_0,
            "",
            "SHARE: 1.0, TOTAL: 7.0",
            duplicate("TOTAL", "aggregate"),
        ),
        // Reading TOTAL is refused at the read, instead of completing the
        // default in the template, where an ungrouped template has no
        // aggregate context (LOOKUP-18) ...
        (
            V0_5,
            "",
            "SHARE: {compute: {expr: \"EX.EXDOSE / TOTAL\"}}",
            boundary("rows[0].derivations.SHARE.compute.expr"),
        ),
        (
            V1_0,
            "",
            "SHARE: \"EX.EXDOSE / TOTAL\"",
            boundary("rows.dosed.derivations.SHARE"),
        ),
        // ... and a grouped one reduced over its own group, not over EX's
        // records for the subject.
        (
            V0_5,
            grouped,
            "SHARE: {compute: {expr: \"TOTAL / 2\"}}",
            boundary("rows[0].derivations.SHARE.compute.expr"),
        ),
        (
            V1_0,
            grouped,
            "SHARE: \"TOTAL / 2\"",
            boundary("rows.dosed.derivations.SHARE"),
        ),
        // A default that reads a SELF intermediate names it as the reason.
        (
            V0_5,
            "",
            "SHARE: {literal: 1.0}, FIRSTSEQ: {literal: 0}",
            duplicate("FIRSTSEQ", "self_intermediate"),
        ),
        (
            V1_0,
            "",
            "SHARE: 1.0, FIRSTSEQ: 0",
            duplicate("FIRSTSEQ", "self_intermediate"),
        ),
    ] {
        let fixture = Fixture::new(&dosing(spelling, fields, derivations));
        fs::write(
            fixture.0.join("ex.csv"),
            "STUDYID,USUBJID,EXSEQ,EXDOSE\nS,S-1,1,10\nS,S-1,2,20\nS,S-2,1,5\n",
        )
        .unwrap();
        let json = |issues: Vec<Diagnostic>| {
            issues
                .iter()
                .map(crate::support::study::issue_row)
                .collect::<Vec<_>>()
        };
        for host in [Language::Python, Language::R] {
            let (checked, built) = fixture.diagnostics(host);
            let case = format!("{spelling:?} {derivations} ({host:?})");
            assert_eq!(
                json(checked),
                std::slice::from_ref(&expected),
                "check: {case}"
            );
            assert_eq!(
                json(built),
                std::slice::from_ref(&expected),
                "domain: {case}"
            );
        }
    }
}
