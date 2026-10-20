#![cfg(feature = "yaml")]
//! `check` reports the group-scope and driver-scope refusals and a row
//! filter's phase boundaries as `domain` does (#3229, #2844 decision 3).
//! Every case lists the same issues in both routes and hosts, and names the
//! condition and rule of its first issue.
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
            "yamaa-row-scopes-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("spec.yaml"), specification).unwrap();
        for (name, content) in [
            (
                "vs.csv",
                "STUDYID,USUBJID,VSTESTCD,VSDTC,VSDY,SYSBP,DIABP,DEVICEID\nS,S-1,SYSBP,2025-01-01,1,120,80,D1\n",
            ),
            (
                "home.csv",
                "STUDYID,USUBJID,VSTESTCD,VSDTC,VSDY,SYSBP,DIABP\nS,S-1,DIABP,2025-01-03,3,119,79\n",
            ),
            (
                "plan.csv",
                "STUDYID,USUBJID,DOSENUM,PLANDOSE,FIRST,LAST,REF,DEVICEID,METHOD\nS,S-1,1,10,1,2,1,D1,AUTO\n",
            ),
            ("dm.csv", "STUDYID,USUBJID,RFSTDTC\nS,S-1,2025-01-01\n"),
        ] {
            fs::write(root.join(name), content).unwrap();
        }
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
            example: "row-scopes",
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
                        issue.rule(),
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

const INPUTS: &str = "input:\n\
    \x20 VS: {path: vs.csv, types: {VSDTC: date, VSDY: int, SYSBP: float, DIABP: float}}\n\
    \x20 HOME: {path: home.csv, types: {VSDTC: date, VSDY: int, SYSBP: float, DIABP: float}}\n\
    \x20 PLAN: {path: plan.csv, types: {DOSENUM: int, PLANDOSE: float, FIRST: int, LAST: int, REF: int}}\n\
    \x20 DM: {path: dm.csv, types: {RFSTDTC: date}}\n";

/// The output columns, with the defaults of X and C. SEQ is a window, which
/// only the column phase completes; N and M are plain integers.
fn columns(x: &str, c: &str) -> String {
    format!(
        "output: {{path: out.csv, columns: [STUDYID, USUBJID, PARAMCD, SEQ, AVAL, X, C, N, M]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str, derivation: VS.STUDYID}}\n\
         \x20 - {{name: USUBJID, type: str, derivation: VS.USUBJID}}\n\
         \x20 - {{name: PARAMCD, type: str}}\n\
         \x20 - name: SEQ\n\
         \x20   type: int\n\
         \x20   derivation: {{row_number: {{window: {{group_by: [STUDYID, USUBJID], order_by: [PARAMCD]}}}}}}\n\
         \x20 - {{name: AVAL, type: float}}\n\
         \x20 - {{name: X, type: float, derivation: {x}}}\n\
         \x20 - {{name: C, type: str, derivation: {c}}}\n\
         \x20 - {{name: N, type: int, derivation: {{literal: 0}}}}\n\
         \x20 - {{name: M, type: int, derivation: {{literal: 0}}}}\n"
    )
}

/// The parts of a case: its lookups, one more derivation of the (first)
/// template, the template's other fields such as `filter`, and the defaults
/// of X and C.
#[derive(Clone, Copy)]
struct Case {
    intermediates: &'static str,
    derivation: &'static str,
    extra: &'static str,
    x: &'static str,
    c: &'static str,
}

const CASE: Case = Case {
    intermediates: "",
    derivation: "",
    extra: "",
    x: "{literal: 0.0}",
    c: "{literal: none}",
};

fn header(keys: &str, case: Case) -> String {
    format!(
        "schema_version: \"1.0\"\ndomain: ADVS\nkeys: [{keys}]\n{INPUTS}{}{}",
        case.intermediates,
        columns(case.x, case.c)
    )
}

fn extra_derivation(case: Case) -> String {
    if case.derivation.is_empty() {
        String::new()
    } else {
        format!("      {}\n", case.derivation)
    }
}

/// One template over VS, grouped by subject.
fn grouped(case: Case) -> String {
    format!(
        "{}rows:\n  - id: subject\n    dataset: VS\n    group_by: [VS.STUDYID, VS.USUBJID]\n{}    derivations:\n      PARAMCD: {{literal: MAXBP}}\n      AVAL: {{aggregate: \"MAX(VS.SYSBP)\"}}\n{}",
        header("STUDYID, USUBJID, PARAMCD", case),
        case.extra,
        extra_derivation(case)
    )
}

/// One template over VS, a row per record.
fn ungrouped(case: Case) -> String {
    format!(
        "{}rows:\n  - id: record\n    dataset: VS\n{}    derivations:\n      PARAMCD: {{literal: SYSBP}}\n      AVAL: VS.SYSBP\n{}",
        header("STUDYID, USUBJID, PARAMCD, SEQ", case),
        case.extra,
        extra_derivation(case)
    )
}

/// Templates over VS and HOME; `home` is one more derivation of the second.
fn two(case: Case, home: &str) -> String {
    let header = header("STUDYID, USUBJID, PARAMCD, SEQ", case)
        .replace("derivation: VS.STUDYID", "")
        .replace("derivation: VS.USUBJID", "");
    let home = if home.is_empty() {
        String::new()
    } else {
        format!("      {home}\n")
    };
    format!(
        "{header}rows:\n\
         \x20 - id: clinic\n    dataset: VS\n{}    derivations:\n      STUDYID: VS.STUDYID\n      USUBJID: VS.USUBJID\n      PARAMCD: {{literal: CLINIC}}\n      AVAL: VS.SYSBP\n{}\
         \x20 - id: home\n    dataset: HOME\n    derivations:\n      STUDYID: HOME.STUDYID\n      USUBJID: HOME.USUBJID\n      PARAMCD: {{literal: HOME}}\n      AVAL: HOME.SYSBP\n{home}",
        case.extra,
        extra_derivation(case)
    )
}

/// No rows: VS is the base; X derives `column`.
fn implicit(intermediates: &str, column: &str, verification: &str) -> String {
    format!(
        "schema_version: \"1.0\"\ndomain: ADVS\nkeys: [STUDYID, USUBJID, VSDY]\n{INPUTS}base: VS\n{intermediates}\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, VSDY, X]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str, derivation: VS.STUDYID}}\n\
         \x20 - {{name: USUBJID, type: str, derivation: VS.USUBJID}}\n\
         \x20 - {{name: VSDY, type: int, derivation: VS.VSDY}}\n\
         \x20 - {{name: X, type: float, derivation: {column}}}\n\
         {verification}"
    )
}

const PLANNED: &str =
    "intermediates:\n  - {id: PLANNED, dataset: PLAN, key: {DOSENUM: VS.VSDY}, no_match: null}\n";
const PERIOD: &str = "intermediates:\n  - {id: PERIOD, dataset: PLAN, key: [STUDYID], between: {value: VS.VSDY, lower: FIRST, upper: LAST}, no_match: null}\n";
const WINDOWED: &str = "intermediates:\n  - {id: WIN, dataset: PLAN, key: [USUBJID], filter: \"PLAN.FIRST <= PLAN.D\", no_match: null, derivations: {D: VS.VSDY}}\n";
const FILTERED: &str = "intermediates:\n  - {id: WIN, dataset: PLAN, key: [USUBJID], filter: \"PLAN.FIRST <= VS.VSDY\", no_match: null}\n";
const DEVICE: &str =
    "intermediates:\n  - {id: DEVICE, dataset: PLAN, key: {DEVICEID: VS.DEVICEID}, no_match: null}\n";

/// Check each specification in both routes and hosts: the same issues, the
/// first of them the expected condition and rule.
fn assert_cases(cases: &[(String, &str)]) {
    for (specification, first) in cases {
        let fixture = Fixture::new(specification);
        for host in [Language::Python, Language::R] {
            let (checked, built) = fixture.issues(host);
            assert_eq!(checked, built, "{specification}\n({host:?})");
            assert!(
                checked
                    .first()
                    .is_some_and(|issue| issue.starts_with(first)),
                "{specification}\n({host:?}): {checked:?}"
            );
        }
    }
}

#[test]
fn check_reports_each_group_scope_refusal_as_domain_does() {
    let row = |derivation| grouped(Case { derivation, ..CASE });
    let ungrouped_driver = "ungrouped_driver_field Some(\"ROW-5\")";
    assert_cases(&[
        // A driver field read in each kind of row derivation.
        (row("X: VS.SYSBP"), ungrouped_driver),
        (row("X: {compute: {expr: \"VS.SYSBP + 1\"}}"), ungrouped_driver),
        (row("C: {flag: \"VS.SYSBP > 100\"}"), ungrouped_driver),
        (
            row("X: {case: [{when: \"VS.SYSBP > 1\", then: {literal: 1.0}}, {otherwise: {literal: 2.0}}]}"),
            ungrouped_driver,
        ),
        (
            row("X: {case: [{when: \"AVAL > 1\", then: VS.SYSBP}, {otherwise: {literal: 2.0}}]}"),
            ungrouped_driver,
        ),
        (row("X: {round_half_away_from_zero: {source: VS.SYSBP, digits: 0}}"), ungrouped_driver),
        (row("X: {greatest: {sources: [VS.SYSBP, VS.DIABP]}}"), ungrouped_driver),
        (row("X: {first_available: {sources: [VS.SYSBP, VS.DIABP]}}"), ungrouped_driver),
        (row("C: {str_template: \"{VS.VSTESTCD}-{SEQ}-{VS.VSDTC}\"}"), ungrouped_driver),
        (row("C: {mapping: {source: VS.VSTESTCD, dict: {\"1\": one}}}"), ungrouped_driver),
        (row("C: {str_case: {source: VS.VSTESTCD, to: lower}}"), ungrouped_driver),
        (
            row("N: {diff: {type: date, start: VS.VSDTC, end: VS.VSDTC, unit: day}}"),
            ungrouped_driver,
        ),
        (
            row("X: {source: {variable: VS.SYSBP, order_by: [VS.VSDY], keep: first}}"),
            ungrouped_driver,
        ),
        // A read of the row's own column comes before the window's grouping.
        (row("X: {compute: {expr: \"SEQ + VS.SYSBP\"}}"), "phase_boundary Some(\"COLUMN-4\")"),
        // A window's grouping, then its defects, then its reads.
        (
            row("X: {row_value: {source: SEQ, offset: 0, window: {order_by: [VS.VSDY]}}}"),
            ungrouped_driver,
        ),
        // A grouped row's aggregate reads only group keys outside its
        // reductions (FORMULA-22), and its header comes first (LOOKUP-18).
        (
            row("X: {aggregate: \"MAX(VS.SYSBP) + VS.DIABP\"}"),
            "aggregate_identifier_not_grouped Some(\"FORMULA-22\")",
        ),
        (
            row("X: {aggregate: \"MAX(VS.SYSBP) + SEQ\"}"),
            "mixed_relations Some(\"LOOKUP-18\")",
        ),
        // A default the template completes in the row phase.
        (grouped(Case { x: "VS.SYSBP", ..CASE }), ungrouped_driver),
        // Through a lookup's key, range value and derivation (ROW-5), and
        // its filter (LOOKUP-7).
        (
            grouped(Case {
                intermediates: PLANNED,
                derivation: "X: PLANNED.PLANDOSE",
                ..CASE
            }),
            ungrouped_driver,
        ),
        (
            grouped(Case {
                intermediates: PERIOD,
                derivation: "X: PERIOD.PLANDOSE",
                ..CASE
            }),
            ungrouped_driver,
        ),
        (
            grouped(Case {
                intermediates: WINDOWED,
                derivation: "X: WIN.PLANDOSE",
                ..CASE
            }),
            ungrouped_driver,
        ),
        (
            grouped(Case {
                intermediates: FILTERED,
                derivation: "X: WIN.PLANDOSE",
                ..CASE
            }),
            "unknown_field Some(\"LOOKUP-7\")",
        ),
        (
            grouped(Case {
                intermediates: PLANNED,
                x: "PLANNED.PLANDOSE",
                ..CASE
            }),
            ungrouped_driver,
        ),
        // The column phase reads a grouped template's driver only at its
        // group keys (NAME-4).
        (
            grouped(Case {
                c: "{mapping: {source: VS.VSTESTCD, dict: {\"1\": one}}}",
                ..CASE
            }),
            "ungrouped_driver_field Some(\"NAME-4\")",
        ),
        (
            grouped(Case {
                intermediates: PLANNED,
                x: "{case: [{when: \"VS.SYSBP > 1 AND PLANNED.PLANDOSE > 1\", then: {literal: 1.0}}, {otherwise: {literal: 2.0}}]}",
                ..CASE
            }),
            "ungrouped_driver_field Some(\"NAME-4\")",
        ),
        // A later grouped template, with a row-phase read of the first.
        (
            two(
                Case {
                    x: "VS.SYSBP",
                    extra: "    group_by: [VS.STUDYID, VS.USUBJID, VS.VSTESTCD]\n",
                    ..CASE
                },
                "",
            ),
            ungrouped_driver,
        ),
    ]);
}

#[test]
fn check_reports_each_driver_scope_refusal_as_domain_does() {
    let lookup_key = "unknown_field Some(\"LOOKUP-5\")";
    let source_scope = "unknown_field Some(\"FORMULA-9\")";
    assert_cases(&[
        // A lookup read for HOME's rows matches on VS's record: in the
        // column phase, in a row template, through its derivation and its
        // filter.
        (two(Case { intermediates: DEVICE, c: "DEVICE.METHOD", ..CASE }, ""), lookup_key),
        (two(Case { intermediates: DEVICE, ..CASE }, "C: DEVICE.METHOD"), lookup_key),
        (
            two(Case { intermediates: WINDOWED, ..CASE }, "X: WIN.PLANDOSE"),
            "unknown_field Some(\"LOOKUP-15\")",
        ),
        (
            two(Case { intermediates: FILTERED, ..CASE }, "X: WIN.PLANDOSE"),
            "unknown_field Some(\"LOOKUP-7\")",
        ),
        // An aggregate's key value read from VS for every row.
        (
            two(
                Case {
                    x: "{aggregate: {expr: \"COUNT(PLAN.DOSENUM)\", key: {DEVICEID: VS.DEVICEID}}}",
                    ..CASE
                },
                "",
            ),
            lookup_key,
        ),
        // Without row templates, a lookup that a column or a verification
        // reads matches on another input's record.
        (
            implicit(
                "intermediates:\n  - {id: SUBJ, dataset: PLAN, key: {USUBJID: DM.USUBJID}, no_match: null}\n",
                "SUBJ.PLANDOSE",
                "",
            ),
            lookup_key,
        ),
        (
            implicit(
                "intermediates:\n  - {id: SUBJ, dataset: PLAN, key: [USUBJID], filter: \"PLAN.REF IS NOT NULL AND DM.RFSTDTC IS NOT NULL\", no_match: null}\n",
                "SUBJ.PLANDOSE",
                "",
            ),
            "unknown_field Some(\"LOOKUP-7\")",
        ),
        (
            implicit(
                "intermediates:\n  - {id: SUBJ, dataset: PLAN, key: [USUBJID], filter: \"PLAN.USUBJID = PLAN.D\", no_match: null, derivations: {D: DM.USUBJID}}\n",
                "SUBJ.PLANDOSE",
                "",
            ),
            "unknown_field Some(\"LOOKUP-15\")",
        ),
        (
            implicit(
                "intermediates:\n  - {id: SUBJ, dataset: PLAN, key: {USUBJID: DM.USUBJID}, no_match: null}\n",
                "{literal: 1.0}",
                "verifications:\n  - assert: {id: has-plan, require: \"SUBJ.PLANDOSE IS NOT NULL\"}\n",
            ),
            lookup_key,
        ),
        // A row formula, flag or case predicate reads only its driver's
        // record, and a column formula no qualified name but a lookup's.
        (
            ungrouped(Case {
                derivation: "X: {compute: {expr: \"HOME.SYSBP + 1\"}}",
                ..CASE
            }),
            source_scope,
        ),
        (
            ungrouped(Case {
                derivation: "C: {flag: \"HOME.SYSBP > 1\"}",
                ..CASE
            }),
            source_scope,
        ),
        (
            grouped(Case {
                derivation: "X: {case: [{when: \"HOME.SYSBP > 1\", then: {literal: 1.0}}, {otherwise: {literal: 2.0}}]}",
                ..CASE
            }),
            source_scope,
        ),
        (
            two(Case { x: "{compute: {expr: \"VS.SYSBP + 1\"}}", ..CASE }, ""),
            "qualified_identifier Some(\"FORMULA-9\")",
        ),
    ]);
}

#[test]
fn check_reports_each_row_filter_phase_boundary_as_domain_does() {
    let filter = |intermediates, derivation, extra| {
        ungrouped(Case {
            intermediates,
            derivation,
            extra,
            ..CASE
        })
    };
    let row_filter = "phase_boundary Some(\"ROW-6\")";
    let window = "N: {row_number: {window: {order_by: [VS.VSDY]}}}";
    assert_cases(&[
        // A column the template leaves to the column phase.
        (filter("", "", "    filter: \"SEQ > 1\"\n"), row_filter),
        // A window result, directly, through a column, and through a
        // lookup's match.
        (filter("", window, "    filter: \"N > 1\"\n"), row_filter),
        (
            filter("", "N: {row_number: {window: {order_by: [VS.VSDY]}}}\n      M: N", "    filter: \"M > 1\"\n"),
            row_filter,
        ),
        (
            filter(
                "intermediates:\n  - {id: BYN, dataset: PLAN, key: {DOSENUM: N}, no_match: null}\n",
                window,
                "    filter: \"BYN.PLANDOSE IS NULL\"\n",
            ),
            row_filter,
        ),
        // A lookup that matches on a column the template leaves to the
        // column phase (LOOKUP-2), or on another input's record (LOOKUP-5).
        (
            filter(
                "intermediates:\n  - {id: BYSEQ, dataset: PLAN, key: {DOSENUM: SEQ}, no_match: null}\n",
                "",
                "    filter: \"BYSEQ.PLANDOSE IS NULL\"\n",
            ),
            "phase_boundary Some(\"LOOKUP-2\")",
        ),
        (
            filter(
                "intermediates:\n  - {id: SUBJ, dataset: PLAN, key: {USUBJID: DM.USUBJID}, no_match: null}\n",
                "",
                "    filter: \"SUBJ.PLANDOSE IS NOT NULL\"\n",
            ),
            "unknown_field Some(\"LOOKUP-5\")",
        ),
        // A grouped filter's qualified names, before the derivations, then
        // its columns the template does not derive.
        (
            grouped(Case {
                extra: "    filter: \"VS.SYSBP > 1 AND SEQ > 1 AND HOME.SYSBP > 1\"\n",
                derivation: "X: {compute: {expr: \"SEQ + 1.0\"}}",
                ..CASE
            }),
            row_filter,
        ),
        (
            grouped(Case {
                extra: "    filter: \"SEQ > 1\"\n",
                ..CASE
            }),
            row_filter,
        ),
    ]);
}
