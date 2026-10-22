#![cfg(feature = "yaml")]
//! `check` reports, as `domain` does and before any input is read, each read
//! that binding refuses from the declarations alone in four more places
//! (#3542, #2844 decision 3): a root `filter`'s read of another dataset or of
//! a column (NAME-3, ROW-9), reported once every column and the column order
//! are bound; a flag's or `case` predicate's qualifier that nothing declares
//! (FORMULA-9); a formula in a `case` result of a specification without
//! `rows`, which reads only lookups and columns (NAME-3); and each read of a
//! lookup whose key types fail, which binding does not bind. So it does for
//! two more (#3552): a row template filter's field qualified by nothing
//! declared (NAME-3), and the operands a formula reads, typed once its names
//! bind (FORMULA-12); and for two more (#3580): every other read of a field
//! qualified by nothing declared (NAME-3, FORMULA-9, LOOKUP-9), and the
//! reads of a lookup binding binds without its derivations, which it admits
//! for no row (LOOKUP-15); and for one more (#3585): a text extraction's or
//! replacement's source that is not text (TEXT-1) and, once binding takes
//! its source, its pattern and group (TEXT-6, TEXT-10), wherever binding
//! binds it; and for two more (#3678): an aggregate filter's read outside
//! what the aggregate reduces (NAME-3), and a lookup filter's field
//! qualified by nothing declared (LOOKUP-7) where the declarations decide
//! the finding's `suggestion` (D220). Every case lists the same issues in
//! both routes and hosts, in the same order, except where only the records
//! decide a finding: `check` leaves that case to `domain`, whose issues its
//! own stay a subsequence of (D195, D199).
use crate::support::Edit as _;
use std::fs;
use yamaa_adapters::file_application;
use yamaa_core::{diagnostic::Diagnostic, domain_entry::Request, project_function::Language};

/// Rendered issues, each with its spec paths.
type Issues = Vec<(String, Vec<String>)>;

struct Fixture(crate::support::Scratch);
impl Fixture {
    fn new(specification: &str) -> Self {
        let root = crate::support::Scratch::new("unknown-reads");
        fs::write(root.join("spec.yaml"), specification).unwrap();
        for (name, content) in [
            (
                "vs.csv",
                "STUDYID,USUBJID,VSTESTCD,VSDTC,VSDY,SYSBP\nS,S-1,SYSBP,2025-01-01,1,120\n",
            ),
            (
                "home.csv",
                "STUDYID,USUBJID,VSTESTCD,VSDTC,VSDY,SYSBP\nS,S-1,SYSBP,2025-01-03,3,119\n",
            ),
            ("plan.csv", "STUDYID,USUBJID,DOSENUM,PLANDOSE\nS,S-1,1,10\n"),
            (
                "dm.csv",
                "STUDYID,USUBJID,RFSTDTC,AGE\nS,S-1,2025-01-01,40\n",
            ),
            // The producer of ADSL, its own input, and the ADSL it saved.
            (
                "spec_adsl.yaml",
                "yamaa: \"1.0\"\ntype: spec\ndomain: ADSL\nkeys: [STUDYID, USUBJID]\n\
                 input:\n  DM: subjects.csv\n\
                 output:\n  path: adsl.csv\n  columns: [STUDYID, USUBJID, AGE, SEX]\n\
                 columns:\n\
                 \x20 - {name: STUDYID, type: str, label: Study Identifier, derivation: DM.STUDYID}\n\
                 \x20 - {name: USUBJID, type: str, label: Unique Subject Identifier, derivation: DM.USUBJID}\n\
                 \x20 - {name: AGE, type: int, label: Age, derivation: DM.AGE}\n\
                 \x20 - {name: SEX, type: str, label: Sex, derivation: DM.SEX}\n",
            ),
            ("subjects.csv", "STUDYID,USUBJID,AGE,SEX\nS,S-1,40,F\n"),
            ("adsl.csv", "STUDYID,USUBJID,AGE,SEX\nS,S-1,40,F\n"),
        ] {
            fs::write(root.join(name), content).unwrap();
        }
        Self(root)
    }
    /// The issues of `check` and of `domain`, as each host reports them, each
    /// with its spec paths.
    fn issues(&self, host: Language) -> (Issues, Issues) {
        let specification = self.0.join("spec.yaml");
        let specification = specification.to_str().unwrap();
        let request = || Request {
            specification,
            environment: None,
        };
        let identity = || crate::support::study::identity("unknown-reads", specification);
        let render = |issues: &[Diagnostic]| {
            issues
                .iter()
                .map(|issue| {
                    (
                        format!(
                            "{} {:?} {:?} {:?}",
                            issue.condition(),
                            issue.rule(),
                            issue.spec_paths,
                            issue.context
                        ),
                        issue.spec_paths.clone(),
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

/// A specification, the spec paths of its issues in order, and how its
/// first issue starts.
type Case<'a> = (String, &'a [&'a str], &'a str);

/// Check each specification in both routes and hosts: the same issues, in
/// the same order, at `paths`, the first one as given.
fn assert_cases(cases: &[Case<'_>]) {
    for (specification, paths, first) in cases {
        let fixture = Fixture::new(specification);
        for host in [Language::Python, Language::R] {
            let (checked, built) = fixture.issues(host);
            assert_eq!(checked, built, "{specification}\n({host:?})");
            assert!(
                checked
                    .first()
                    .is_some_and(|(issue, _)| issue.starts_with(first)),
                "{specification}\n({host:?}): {checked:?}"
            );
            let listed = checked
                .iter()
                .map(|(_, paths)| paths.join(", "))
                .collect::<Vec<_>>();
            assert_eq!(listed, *paths, "{specification}\n({host:?}): {checked:?}");
        }
    }
}

/// Check each specification in both routes and hosts: neither reports an
/// issue.
fn assert_clean(specifications: &[String]) {
    for specification in specifications {
        let fixture = Fixture::new(specification);
        for host in [Language::Python, Language::R] {
            let (checked, built) = fixture.issues(host);
            assert_eq!(checked, built, "{specification}\n({host:?})");
            assert!(
                checked.is_empty(),
                "{specification}\n({host:?}): {checked:?}"
            );
        }
    }
}

/// Check each specification in both routes and hosts where only the records
/// decide a finding: `check`'s issues, at `checked`, are an exact
/// subsequence of `domain`'s, at `built` (D195, D199, D220).
fn assert_subsequence(cases: &[(String, &[&str], &[&str])]) {
    for (specification, checked_paths, built_paths) in cases {
        let fixture = Fixture::new(specification);
        for host in [Language::Python, Language::R] {
            let (checked, built) = fixture.issues(host);
            let paths = |issues: &Issues| {
                issues
                    .iter()
                    .map(|(_, paths)| paths.join(", "))
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                paths(&checked),
                *checked_paths,
                "{specification}\n({host:?}): {checked:?}"
            );
            assert_eq!(
                paths(&built),
                *built_paths,
                "{specification}\n({host:?}): {built:?}"
            );
            let mut remaining = built.iter();
            assert!(
                checked
                    .iter()
                    .all(|issue| remaining.any(|other| other == issue)),
                "{specification}\n({host:?}): {checked:?} within {built:?}"
            );
        }
    }
}

const NAME: &str = "unknown_field Some(\"NAME-3\")";
const PREDICATE: &str = "unknown_field Some(\"FORMULA-9\")";
const KEY_TYPES: &str = "incompatible_input_type Some(\"LOOKUP-9\") [\"intermediates.PL.key\"]";
/// The first issue of every read of PL.
const PL: &str = "intermediates.PL.key";

/// The inputs, HOME without `key`, and the lookups of PLAN: PL, whose key
/// types fail (DOSENUM, an int, matches the str USUBJID), and PB, a filtered
/// lookup that binds; `lookups` adds more.
fn declarations(lookups: &str) -> String {
    format!(
        "input:\n\
         \x20 VS: {{path: vs.csv, types: {{VSDTC: date, VSDY: int, SYSBP: float}}}}\n\
         \x20 HOME: {{path: home.csv, types: {{VSDTC: date, VSDY: int, SYSBP: float}}}}\n\
         \x20 PLAN: {{path: plan.csv, key: [USUBJID], types: {{DOSENUM: int, PLANDOSE: float}}}}\n\
         \x20 DM: {{path: dm.csv, key: [USUBJID], types: {{RFSTDTC: date, AGE: int}}}}\n\
         intermediates:\n\
         \x20 - {{id: PL, dataset: PLAN, key: {{DOSENUM: VS.USUBJID}}}}\n\
         \x20 - {{id: PB, dataset: PLAN, key: [USUBJID], filter: \"PLAN.DOSENUM > 0\"}}\n{lookups}"
    )
}

/// No rows: VS is the base, `root` holds more root fields (a `filter`), and
/// the float X and the str C derive `x` and `c`.
fn implicit(lookups: &str, root: &str, x: &str, c: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADVS\nkeys: [STUDYID, USUBJID, VSDY]\n{}base: VS\n{root}\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, VSDY, X, C]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str, derivation: VS.STUDYID}}\n\
         \x20 - {{name: USUBJID, type: str, derivation: VS.USUBJID}}\n\
         \x20 - {{name: VSDY, type: int, derivation: VS.VSDY}}\n\
         \x20 - {{name: X, type: float, derivation: {x}}}\n\
         \x20 - {{name: C, type: str, derivation: {c}}}\n",
        declarations(lookups)
    )
}

/// One row template over VS, with `filter` and the derivations
/// `derivations` besides its own; the float X and the str C default to `x`
/// and `c`.
fn record(filter: &str, derivations: &str, x: &str, c: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADVS\nkeys: [STUDYID, USUBJID, VSDY]\n{}\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, VSDY, X, C]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str}}\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: VSDY, type: int}}\n\
         \x20 - {{name: X, type: float, derivation: {x}}}\n\
         \x20 - {{name: C, type: str, derivation: {c}}}\n\
         rows:\n\
         \x20 - id: record\n    dataset: VS\n{filter}    derivations:\n\
         \x20     STUDYID: VS.STUDYID\n      USUBJID: VS.USUBJID\n      VSDY: VS.VSDY\n{derivations}",
        declarations("")
    )
}

/// The float and str defaults no case reads.
const X: &str = "0.0";
const C: &str = "\"'A'\"";

/// A root `filter` reads only the base's records, before any column has a
/// value: binding refuses a field of any other dataset (NAME-3) and a bare
/// name (ROW-9), each once, once every column and the column order are
/// bound.
#[test]
fn check_reports_a_root_filter_s_reads_as_domain_does() {
    let filtered = |filter: &str, x: &str| implicit("", &format!("filter: \"{filter}\"\n"), x, C);
    assert_cases(&[
        (filtered("DM.AGE > 1", X), &["filter"], NAME),
        (
            filtered("USUBJID = 'x'", X),
            &["filter"],
            "phase_boundary Some(\"ROW-9\")",
        ),
        (
            filtered("DM.AGE > 1 AND XX.Y > 2 AND DM.AGE < 9 AND VS.SYSBP > 0", X),
            &["filter", "filter"],
            NAME,
        ),
        // After a column's refusal, its grammar too.
        (
            filtered("DM.AGE > 1", "HOME.SYSBP"),
            &["columns.X.derivation.source", "filter"],
            "no_applicable_keys Some(\"LOOKUP-5\")",
        ),
        (
            filtered("VS.SYSBP >", "HOME.SYSBP"),
            &["columns.X.derivation.source", "filter"],
            "no_applicable_keys Some(\"LOOKUP-5\")",
        ),
    ]);
    assert_clean(&[filtered("VS.SYSBP > 1", "DM.AGE")]);
}

/// Without row templates, a flag's or `case` predicate's name qualified by
/// nothing an input or a lookup is, is unknown to it (FORMULA-9).
#[test]
fn check_reports_a_predicate_s_undeclared_qualifier_as_domain_does() {
    assert_cases(&[
        (
            implicit("", "", X, "{flag: \"VS.SYSBP > 1 AND XX.AGE > 1\"}"),
            &["columns.C.derivation.flag.condition"],
            PREDICATE,
        ),
        (
            implicit(
                "",
                "",
                "{case: [{when: \"XX.AGE > 1\", then: 1.0}, {otherwise: 2.0}]}",
                C,
            ),
            &["columns.X.derivation.case[0].when"],
            PREDICATE,
        ),
    ]);
    assert_clean(&[implicit(
        "",
        "",
        "{case: [{when: \"PB.PLANDOSE > 1\", then: 1.0}, {otherwise: 2.0}]}",
        "{flag: \"DM.AGE > 1 AND PB.PLANDOSE > 1\"}",
    )]);
}

/// Without row templates, a formula in a `case` result reads only lookups
/// and columns: each other name, once, is unknown (NAME-3), also in a
/// nested `case`.
#[test]
fn check_reports_a_case_result_formula_s_reads_as_domain_does() {
    let result = |then: &str| {
        implicit(
            "",
            "",
            &format!("{{case: [{{when: \"VS.SYSBP > 1\", then: {then}}}, {{otherwise: 0.0}}]}}"),
            C,
        )
    };
    let at = "columns.X.derivation.case[0].then";
    assert_cases(&[
        (result("\"DM.AGE + 1\""), &[at], NAME),
        (result("\"VS.SYSBP + NOPE + NOPE\""), &[at, at], NAME),
        (
            result("{case: [{when: \"VSDY > 1\", then: \"DM.AGE + 1\"}, {otherwise: 0.0}]}"),
            &["columns.X.derivation.case[0].then.case[0].then"],
            NAME,
        ),
    ]);
    assert_clean(&[result("\"VSDY + PB.PLANDOSE\"")]);
}

/// Binding binds nothing of a lookup whose key types fail (LOOKUP-9), so
/// each read of it fails where it is read: as unknown (NAME-3) at a name, a
/// structured source (before its filter), an operand, a placeholder, a
/// mapping or `first_available` source, a formula and a row filter; as
/// unknown to a flag or `case` predicate (FORMULA-9); and as no value of a
/// range (LOOKUP-9). A formula reports its other qualified names first
/// (FORMULA-9).
#[test]
fn check_reports_each_read_of_a_lookup_whose_key_types_fail_as_domain_does() {
    let column = |x: &str, c: &str| implicit("", "", x, c);
    let row = |derivation: &str| record("", &format!("      X: {derivation}\n"), X, C);
    assert_cases(&[
        // Without row templates.
        (column("PL.PLANDOSE", C), &[PL, "columns.X.derivation.source"], KEY_TYPES),
        (
            column(
                "{source: {variable: PL.PLANDOSE, filter: \"PLAN.DOSENUM > 1\", absent: 0.0}}",
                C,
            ),
            &[PL, "columns.X.derivation.source"],
            KEY_TYPES,
        ),
        (
            column("{greatest: {sources: [VS.SYSBP, PL.PLANDOSE]}}", C),
            &[PL, "columns.X.derivation.greatest.sources[1]"],
            KEY_TYPES,
        ),
        (
            column(X, "{str_template: \"{PL.USUBJID}\"}"),
            &[PL, "columns.C.derivation.str_template.template"],
            KEY_TYPES,
        ),
        (
            column(X, "{mapping: {source: PL.USUBJID, dict: {\"1\": one}}}"),
            &[PL, "columns.C.derivation.mapping.source"],
            KEY_TYPES,
        ),
        (
            column("{first_available: {sources: [PL.PLANDOSE, VS.SYSBP]}}", C),
            &[PL, "columns.X.derivation.first_available.sources[0]"],
            KEY_TYPES,
        ),
        (
            column("\"PL.PLANDOSE + DM.AGE + PL.DOSENUM\"", C),
            &[
                PL,
                "columns.X.derivation",
                "columns.X.derivation",
                "columns.X.derivation",
            ],
            KEY_TYPES,
        ),
        (
            column(
                "{case: [{when: \"VS.SYSBP > 1\", then: \"PL.PLANDOSE + 1\"}, {otherwise: 0.0}]}",
                C,
            ),
            &[PL, "columns.X.derivation.case[0].then"],
            KEY_TYPES,
        ),
        (
            column(X, "{flag: \"PL.PLANDOSE > 1\"}"),
            &[PL, "columns.C.derivation.flag.condition"],
            KEY_TYPES,
        ),
        (
            column(
                "{case: [{when: \"PL.PLANDOSE > 1\", then: 1.0}, {otherwise: 2.0}]}",
                C,
            ),
            &[PL, "columns.X.derivation.case[0].when"],
            KEY_TYPES,
        ),
        (
            column(
                "{source: {variable: DM.AGE, between: {value: PL.DOSENUM, lower: AGE}}}",
                C,
            ),
            &[PL, "columns.X.derivation.source.between.value"],
            KEY_TYPES,
        ),
        // Another lookup's range.
        (
            implicit(
                "  - {id: HR, dataset: HOME, key: [USUBJID], between: {value: PL.DOSENUM, lower: VSDY}}\n",
                "",
                "HR.SYSBP",
                C,
            ),
            &[PL, "intermediates.HR.between.value"],
            KEY_TYPES,
        ),
        // A row template's derivations and filter, and the column phase.
        (
            row("PL.PLANDOSE"),
            &[PL, "rows.record.derivations.X.source"],
            KEY_TYPES,
        ),
        (
            row("\"PL.PLANDOSE + 1\""),
            &[PL, "rows.record.derivations.X"],
            KEY_TYPES,
        ),
        (
            row("{case: [{when: \"PL.PLANDOSE > 1\", then: 1.0}, {otherwise: 2.0}]}"),
            &[PL, "rows.record.derivations.X.case[0].when"],
            KEY_TYPES,
        ),
        (
            row("{source: {variable: DM.AGE, between: {value: PL.DOSENUM, lower: AGE}}}"),
            &[PL, "rows.record.derivations.X.source.between.value"],
            KEY_TYPES,
        ),
        (
            record("    filter: \"PL.PLANDOSE > 1\"\n", "", X, C),
            &[PL, "rows.record.filter"],
            KEY_TYPES,
        ),
        (
            record("", "", X, "{str_template: \"{PL.USUBJID}\"}"),
            &[PL, "columns.C.derivation.str_template.template"],
            KEY_TYPES,
        ),
        (
            record(
                "",
                "",
                X,
                "{case: [{when: \"PL.PLANDOSE > 1\", then: \"'A'\"}, {otherwise: \"'B'\"}]}",
            ),
            &[PL, "columns.C.derivation.case[0].when"],
            KEY_TYPES,
        ),
    ]);
    // PB's key types compare: the same reads bind.
    assert_clean(&[
        column(
            "{greatest: {sources: [VS.SYSBP, PB.PLANDOSE]}}",
            "{str_template: \"{PB.USUBJID}\"}",
        ),
        column(
            "{case: [{when: \"PB.PLANDOSE > 1\", then: \"PB.PLANDOSE + 1\"}, {otherwise: 0.0}]}",
            "{flag: \"PB.PLANDOSE > 1\"}",
        ),
        record(
            "    filter: \"PB.PLANDOSE > 1\"\n",
            "      X: \"PB.PLANDOSE + 1\"\n",
            X,
            "{str_template: \"{PB.USUBJID}\"}",
        ),
    ]);
}

const OPERAND: &str = "incompatible_input_type Some(\"FORMULA-12\")";

/// Without row templates a formula reads columns and lookups, and binding
/// types its operands once its names bind, over the names that bound
/// (FORMULA-12): a column's declared type and a literal's, each operand once,
/// after its qualified names (FORMULA-9) and its reads of a lookup binding
/// does not bind (NAME-3). A `case` result is typed after its own names and
/// after its predicate's types. A derivation whose operand fails converts
/// nothing (VALUE-24).
#[test]
fn check_types_a_formula_s_operands_without_rows_as_domain_does() {
    let column = |x: &str| implicit("", "", x, C);
    let result = |then: &str| {
        column(&format!(
            "{{case: [{{when: \"VS.SYSBP > 1\", then: {then}}}, {{otherwise: 0.0}}]}}"
        ))
    };
    let at = "columns.X.derivation";
    let then = "columns.X.derivation.case[0].then";
    assert_cases(&[
        (column("\"USUBJID + 1\""), &[at], OPERAND),
        (column("\"'a' + 1\""), &[at], OPERAND),
        (
            column("\"USUBJID + STUDYID + USUBJID\""),
            &[at, at],
            OPERAND,
        ),
        (
            column("\"DM.AGE + USUBJID\""),
            &[at, at],
            "qualified_identifier Some(\"FORMULA-9\")",
        ),
        (
            column("\"PL.PLANDOSE + USUBJID\""),
            &[PL, at, at],
            KEY_TYPES,
        ),
        (
            implicit("", "", X, "\"USUBJID + 1\""),
            &["columns.C.derivation"],
            OPERAND,
        ),
        (result("\"USUBJID + 1\""), &[then], OPERAND),
        (result("\"DM.AGE + USUBJID\""), &[then, then], NAME),
        (
            result("{case: [{when: \"VSDY > 1\", then: \"USUBJID + 1\"}, {otherwise: 0.0}]}"),
            &["columns.X.derivation.case[0].then.case[0].then"],
            OPERAND,
        ),
        (
            column("{case: [{when: \"USUBJID > 1\", then: \"USUBJID + 1\"}, {otherwise: 0.0}]}"),
            &["columns.X.derivation.case[0].when", then],
            "incompatible_input_type Some(\"FORMULA-5\")",
        ),
        // The 0.5 surface types every name of its `compute` text.
        (
            "schema_version: \"1.0\"\ndomain: ADVS\nkeys: [STUDYID, USUBJID]\n\
             input: {VS: {path: vs.csv, types: {VSDY: int}}}\n\
             output: {path: out.csv, columns: [STUDYID, USUBJID, X]}\n\
             columns:\n\
             \x20 - {name: STUDYID, type: str, derivation: VS.STUDYID}\n\
             \x20 - {name: USUBJID, type: str, derivation: VS.USUBJID}\n\
             \x20 - {name: X, type: float, derivation: {compute: {expr: \"USUBJID + 1\"}}}\n"
                .into(),
            &["columns.X.derivation.compute"],
            OPERAND,
        ),
    ]);
    assert_clean(&[column("\"VSDY + 1.5\""), result("\"VSDY * PB.PLANDOSE\"")]);
}

/// Under row templates a formula's operands are typed the same way: in the
/// row phase over its template's columns and its driver's fields, in the
/// column phase over columns, after its names (FORMULA-9). A flag is typed
/// once its names bind, where binding types it, and a `case` predicate,
/// which binding refuses only once it evaluates it, adds nothing to a
/// derivation that failed.
#[test]
fn check_types_a_formula_s_operands_under_row_templates_as_domain_does() {
    let row = |derivation: &str| record("", &format!("      X: {derivation}\n"), X, C);
    let at = "rows.record.derivations.X";
    let then = "rows.record.derivations.X.case[0].then";
    assert_cases(&[
        (row("\"VS.VSTESTCD + 1\""), &[at], OPERAND),
        (row("\"USUBJID + 1\""), &[at], OPERAND),
        (
            row("\"VS.VSTESTCD + DM.AGE\""),
            &[at, at],
            "unknown_field Some(\"FORMULA-9\")",
        ),
        (
            row("{case: [{when: \"VS.SYSBP > 1\", then: \"VS.VSTESTCD + 1\"}, {otherwise: 1.0}]}"),
            &[then],
            OPERAND,
        ),
        (
            row("{case: [{when: \"VS.VSTESTCD > 1\", then: \"VS.VSTESTCD + 1\"}, {otherwise: 1.0}]}"),
            &[then],
            OPERAND,
        ),
        (
            record("", "      C: \"VS.VSTESTCD + 1\"\n", X, C),
            &["rows.record.derivations.C"],
            OPERAND,
        ),
        (
            record(
                "",
                "      X: {flag: {condition: \"VS.VSTESTCD + 1 > 2\", true_value: 1.0, false_value: 0.0, missing: 0.0}}\n      C: HOME.VSTESTCD\n",
                X,
                C,
            ),
            &[
                "rows.record.derivations.X.flag.condition",
                "rows.record.derivations.C.source",
            ],
            OPERAND,
        ),
        // The column phase.
        (record("", "", "\"USUBJID + 1\"", C), &["columns.X.derivation"], OPERAND),
        (
            record(
                "",
                "",
                "{case: [{when: \"USUBJID = 'a'\", then: \"USUBJID + 1\"}, {otherwise: 1.0}]}",
                C,
            ),
            &["columns.X.derivation.case[0].then"],
            OPERAND,
        ),
    ]);
    assert_clean(&[
        row("\"VS.SYSBP + VS.VSDY\""),
        record("", "", "\"VSDY * 2\"", C),
    ]);
}

/// A lookup's derivation and an aggregate's derive binding type a formula's
/// operands over the donor's records, as binding's captured types type them:
/// the donor's stored fields and the derivations before it; and a flag's
/// condition once its names bind. A typed derivation whose operand fails
/// converts nothing (VALUE-24).
#[test]
fn check_types_a_formula_s_operands_over_a_donor_s_records_as_domain_does() {
    let lookup = |derivations: &str, x: &str, c: &str| {
        implicit(
            &format!(
                "  - {{id: P, dataset: PLAN, key: [USUBJID], derivations: {{{derivations}}}}}\n"
            ),
            "",
            x,
            c,
        )
    };
    let derive = |derivations: &str| {
        implicit(
            "",
            "",
            &format!("{{aggregate: {{expr: \"COUNT(F)\", derivations: {{{derivations}}}}}}}"),
            C,
        )
    };
    let at = "intermediates.P.derivations.D";
    let binding = "columns.X.derivation.aggregate.derivations.F.derivation";
    assert_cases(&[
        (lookup("D: \"PLAN.USUBJID + 1\"", "P.D", C), &[at], OPERAND),
        (lookup("D: \"USUBJID + 1\"", "P.D", C), &[at], OPERAND),
        (
            lookup("E: {str_template: \"{USUBJID}\"}, D: \"E + 1\"", "P.D", C),
            &[at],
            OPERAND,
        ),
        (
            lookup("D: {type: str, derivation: \"USUBJID + 1\"}", X, "P.D"),
            &["intermediates.P.derivations.D.derivation"],
            OPERAND,
        ),
        (
            lookup("D: {flag: \"USUBJID + 1 > 2\"}", X, "P.D"),
            &["intermediates.P.derivations.D.flag.condition"],
            OPERAND,
        ),
        (
            derive("F: {type: str, derivation: \"PLAN.USUBJID + 1\"}"),
            &[binding],
            OPERAND,
        ),
        (
            derive(
                "G: {type: str, derivation: PLAN.USUBJID}, F: {type: float, derivation: \"G + 1\"}",
            ),
            &[binding],
            OPERAND,
        ),
        (
            derive("F: {type: str, derivation: {flag: \"PLAN.USUBJID + 1 > 2\"}}"),
            &["columns.X.derivation.aggregate.derivations.F.derivation.flag.condition"],
            OPERAND,
        ),
    ]);
    assert_clean(&[
        lookup("D: \"PLAN.DOSENUM + 1\"", "P.D", C),
        derive("F: {type: float, derivation: \"PLAN.PLANDOSE + 1\"}"),
    ]);
}

/// An ungrouped row template's filter reads its driver's record, lookups and
/// other inputs through their joins: a field qualified by nothing declared
/// is unknown there (NAME-3), each name once, in written order, after the
/// template's derivations.
#[test]
fn check_reports_a_row_filter_s_undeclared_qualifier_as_domain_does() {
    let filtered = |filter: &str, derivations: &str| {
        record(&format!("    filter: \"{filter}\"\n"), derivations, X, C)
    };
    let at = "rows.record.filter";
    assert_cases(&[
        (filtered("XX.Y > 1", ""), &[at], NAME),
        (
            filtered("NOTDERIVED > 1 AND XX.Y > 1 AND XX.Y < 3 AND YY.Z > 0", ""),
            &[at, at, at],
            "phase_boundary Some(\"ROW-6\")",
        ),
        (
            filtered("HOME.SYSBP > 1 AND XX.Y > 1", ""),
            &[at, at],
            "no_applicable_keys Some(\"LOOKUP-5\")",
        ),
        (
            filtered("XX.Y > 1", "      X: \"VS.VSTESTCD + 1\"\n"),
            &["rows.record.derivations.X", at],
            OPERAND,
        ),
        // The 0.5 surface.
        (
            "schema_version: \"1.0\"\ndomain: ADVS\nkeys: [STUDYID, USUBJID]\n\
             input: {VS: vs.csv}\n\
             output: {path: out.csv, columns: [STUDYID, USUBJID]}\n\
             columns:\n\
             \x20 - {name: STUDYID, type: str}\n\
             \x20 - {name: USUBJID, type: str}\n\
             rows:\n\
             \x20 - {id: vs, dataset: VS, filter: \"XX.Y > 1\", derivations: {STUDYID: VS.STUDYID, USUBJID: VS.USUBJID}}\n"
                .into(),
            &["rows[0].filter"],
            NAME,
        ),
    ]);
    assert_clean(&[filtered(
        "VS.SYSBP > 1 AND PB.PLANDOSE > 1 AND DM.AGE > 1",
        "",
    )]);
}

const RANGE: &str = "unknown_field Some(\"LOOKUP-9\")";

/// A field qualified by nothing declared, no input, lookup or driver, binds
/// nothing whatever any input stores, so binding refuses each read of one
/// where it reads it: as unknown (NAME-3) at a name, a structured source, a
/// verb's operand or placeholder, a `first_available` source and a formula,
/// in a row template, the column phase and without `rows`; as unknown to a
/// `case` predicate (FORMULA-9); and as no value of the range of a
/// structured source, a lookup or an aggregate (LOOKUP-9), as a field of a
/// lookup binding does not bind is none. Outside the implicit template a
/// structured source's filter reads only its variable's record, so each name
/// it qualifies as the variable is unknown there first, once, and over a
/// lookup's records each is the lookup's read (LOOKUP-15).
#[test]
fn check_reports_a_read_qualified_by_nothing_declared_as_domain_does() {
    let row = |derivation: &str| record("", &format!("      X: {derivation}\n"), X, C);
    let text = |derivation: &str| record("", &format!("      C: {derivation}\n"), X, C);
    let column = |x: &str, c: &str| implicit("", "", x, c);
    let at = "rows.record.derivations.X";
    let source = "rows.record.derivations.X.source";
    let filter = "rows.record.derivations.X.source.filter";
    let then = "rows.record.derivations.X.case[0].then";
    let template = "rows.record.derivations.C.str_template.template";
    let between = "source: {variable: DM.AGE, between: {value: XX.Y, lower: AGE}}";
    let filtered =
        "source: {variable: XX.Y, filter: \"XX.Z > 1 AND DM.AGE > 1 AND XX.Z < 9\", absent: 0.0}";
    assert_cases(&[
        // A row template's derivations.
        (row("\"XX.Y + VS.VSTESTCD + XX.Y\""), &[at, at], NAME),
        (row("XX.Y"), &[source], NAME),
        (row("SELF.Y"), &[source], NAME),
        (row("{source: {variable: XX.Y, absent: 0.0}}"), &[source], NAME),
        (
            row("{case: [{when: \"XX.Y > 1\", then: 1.0}, {otherwise: 0.0}]}"),
            &["rows.record.derivations.X.case[0].when"],
            PREDICATE,
        ),
        (
            row("{case: [{when: \"VS.SYSBP > 1\", then: \"XX.Y + VS.VSTESTCD\"}, {otherwise: 0.0}]}"),
            &[then, then],
            NAME,
        ),
        (
            row("{greatest: {sources: [VS.SYSBP, XX.Y]}}"),
            &["rows.record.derivations.X.greatest.sources[1]"],
            NAME,
        ),
        (
            row("{first_available: {sources: [XX.Y, VS.SYSBP]}}"),
            &["rows.record.derivations.X.first_available.sources[0]"],
            NAME,
        ),
        (
            text("{str_template: \"{XX.Y}-{VS.USUBJID}-{YY.Z}\"}"),
            &[template, template],
            NAME,
        ),
        (
            text("{str_extract: {source: XX.Y, pattern: \"^(A)\", group: 1}}"),
            &["rows.record.derivations.C.str_extract.source"],
            NAME,
        ),
        (
            text("{mapping: {source: XX.Y, dict: {\"1\": one}}}"),
            &["rows.record.derivations.C.mapping.source"],
            NAME,
        ),
        (
            row(&format!("{{{between}}}")),
            &["rows.record.derivations.X.source.between.value"],
            RANGE,
        ),
        (
            row(&format!("{{{filtered}}}")),
            &[filter, filter, source],
            NAME,
        ),
        // The column phase.
        (
            record("", "", "XX.Y", C),
            &["columns.X.derivation.source"],
            NAME,
        ),
        (
            record("", "", "{source: {variable: XX.Y, absent: 0.0}}", C),
            &["columns.X.derivation.source"],
            NAME,
        ),
        (
            record(
                "",
                "",
                "{case: [{when: \"XX.Y > 1\", then: 1.0}, {otherwise: 0.0}]}",
                C,
            ),
            &["columns.X.derivation.case[0].when"],
            PREDICATE,
        ),
        (
            record("", "", X, "{str_template: \"{XX.Y}\"}"),
            &["columns.C.derivation.str_template.template"],
            NAME,
        ),
        (
            record("", "", &format!("{{{between}}}"), C),
            &["columns.X.derivation.source.between.value"],
            RANGE,
        ),
        (
            record("", "", &format!("{{{filtered}}}"), C),
            &[
                "columns.X.derivation.source.filter",
                "columns.X.derivation.source.filter",
                "columns.X.derivation.source",
            ],
            NAME,
        ),
        // Without rows the variable fails before any filter binds.
        (column("XX.Y", C), &["columns.X.derivation.source"], NAME),
        (column("SELF.Y", C), &["columns.X.derivation.source"], NAME),
        (
            column(&format!("{{{filtered}}}"), C),
            &["columns.X.derivation.source"],
            NAME,
        ),
        (
            column("{greatest: {sources: [VS.SYSBP, XX.Y]}}", C),
            &["columns.X.derivation.greatest.sources[1]"],
            NAME,
        ),
        (
            column("{first_available: {sources: [XX.Y, VS.SYSBP]}}", C),
            &["columns.X.derivation.first_available.sources[0]"],
            NAME,
        ),
        (
            column(X, "{str_template: \"{XX.Y}\"}"),
            &["columns.C.derivation.str_template.template"],
            NAME,
        ),
        (
            column(
                "{case: [{when: \"VS.SYSBP > 1\", then: XX.Y}, {otherwise: 0.0}]}",
                C,
            ),
            &["columns.X.derivation.case[0].then.source"],
            NAME,
        ),
        (
            column(&format!("{{{between}}}"), C),
            &["columns.X.derivation.source.between.value"],
            RANGE,
        ),
        // A lookup's range and an aggregate's, whose value no lookup binding
        // binds gives either.
        (
            implicit(
                "  - {id: HR, dataset: HOME, key: [USUBJID], between: {value: XX.Y, lower: VSDY}}\n",
                "",
                "HR.SYSBP",
                C,
            ),
            &["intermediates.HR.between.value"],
            RANGE,
        ),
        (
            column(
                "{aggregate: {expr: \"SUM(PLAN.PLANDOSE)\", between: {value: XX.Y, lower: DOSENUM}}}",
                C,
            ),
            &["columns.X.derivation.aggregate.between.value"],
            RANGE,
        ),
        (
            column(
                "{aggregate: {expr: \"SUM(PLAN.PLANDOSE)\", between: {value: PL.DOSENUM, lower: DOSENUM}}}",
                "PL.USUBJID",
            ),
            &[
                PL,
                "columns.X.derivation.aggregate.between.value",
                "columns.C.derivation.source",
            ],
            KEY_TYPES,
        ),
        // Over a lookup's records.
        (
            implicit(
                &format!(
                    "  - {{id: P, dataset: PLAN, key: [USUBJID], derivations: {{D: {{{filtered}}}}}}}\n"
                ),
                "",
                "P.D",
                C,
            ),
            &[
                "intermediates.P.derivations.D.source.filter",
                "intermediates.P.derivations.D.source.filter",
                "intermediates.P.derivations.D.source",
            ],
            "unknown_field Some(\"LOOKUP-15\")",
        ),
        // The 0.5 surface.
        (
            "schema_version: \"1.0\"\ndomain: ADVS\nkeys: [STUDYID, USUBJID]\n\
             input: {VS: vs.csv}\n\
             output: {path: out.csv, columns: [STUDYID, USUBJID]}\n\
             columns:\n\
             \x20 - {name: STUDYID, type: str}\n\
             \x20 - {name: USUBJID, type: str}\n\
             rows:\n\
             \x20 - {id: vs, dataset: VS, derivations: {STUDYID: VS.STUDYID, USUBJID: XX.Y}}\n"
                .into(),
            &["rows[0].derivations.USUBJID.source"],
            NAME,
        ),
        (
            "schema_version: \"1.0\"\ndomain: ADVS\nkeys: [STUDYID, USUBJID]\n\
             input: {VS: vs.csv, PLAN: {path: plan.csv, types: {DOSENUM: int, PLANDOSE: float}}}\n\
             base: VS\n\
             output: {path: out.csv, columns: [STUDYID, USUBJID, X]}\n\
             columns:\n\
             \x20 - {name: STUDYID, type: str, derivation: VS.STUDYID}\n\
             \x20 - {name: USUBJID, type: str, derivation: XX.Y}\n\
             \x20 - name: X\n\
             \x20   type: float\n\
             \x20   derivation: {aggregate: {expr: \"SUM(PLAN.PLANDOSE)\", between: {value: XX.Y, lower: DOSENUM}}}\n"
                .into(),
            &[
                "columns.USUBJID.derivation.source",
                "columns.X.derivation.aggregate.between.value",
            ],
            NAME,
        ),
    ]);
    // The same reads of a declared record bind.
    assert_clean(&[
        row("{source: {variable: DM.AGE, between: {value: VS.VSDY, lower: AGE}}}"),
        row("{source: {variable: VS.SYSBP, filter: \"VS.VSDY > 1\", absent: 0.0}}"),
        record(
            "",
            "",
            "{first_available: {sources: [PB.PLANDOSE, DM.AGE]}}",
            C,
        ),
        column(
            "{aggregate: {expr: \"SUM(PLAN.PLANDOSE)\", between: {value: VSDY, lower: DOSENUM}}}",
            "{str_template: \"{VS.USUBJID}-{DM.STUDYID}\"}",
        ),
    ]);
}

/// Binding binds a lookup without its derivations when one of them fails,
/// whether by a read, an operand's type or a conversion, and when a lookup
/// derived before it is not bound (`intermediate_derivations::bind`). It
/// then admits none of their reads for a row that reads the lookup, so a
/// read of another input's record there is no finding (LOOKUP-15), and a
/// uniqueness verification of the lookup depends on no such read
/// (LOOKUP-17).
#[test]
fn check_admits_no_read_of_a_lookup_bound_without_its_derivations_as_domain_does() {
    let lookup = |derivations: &str| {
        format!("  - {{id: P, dataset: PLAN, key: [USUBJID], derivations: {{{derivations}}}}}\n")
    };
    let unique = |derivations: &str| {
        format!(
            "  - {{id: P, dataset: PLAN, key: [USUBJID], derivations: {{{derivations}}}, verifications: [{{unique: [USUBJID]}}]}}\n"
        )
    };
    let at = "intermediates.P.derivations.D";
    let current = "unknown_field Some(\"LOOKUP-15\")";
    assert_cases(&[
        (
            implicit(
                &lookup("D: {str_template: \"{DOSENUM}\"}, E: DM.RFSTDTC"),
                "",
                X,
                "P.D",
            ),
            &["intermediates.P.derivations.D.str_template.template"],
            "incompatible_input_type Some(\"TEXT-1\")",
        ),
        (
            implicit(&lookup("D: \"DM.RFSTDTC + 1\""), "", "P.D", C),
            &[at],
            OPERAND,
        ),
        (
            implicit(
                &lookup("D: {type: int, derivation: DM.RFSTDTC}"),
                "",
                "P.D",
                C,
            ),
            &[at],
            "conversion_failed Some(\"VALUE-24\")",
        ),
        // PL, whose key types fail, is derived before P.
        (
            implicit(&lookup("E: DM.RFSTDTC"), "", "PL.PLANDOSE", "P.E"),
            &[PL, "columns.X.derivation.source"],
            KEY_TYPES,
        ),
        // A uniqueness verification of the lookup.
        (
            implicit(&unique("D: \"DM.RFSTDTC + 1\""), "", "P.D", C),
            &[at],
            OPERAND,
        ),
        (
            implicit(&unique("E: DM.RFSTDTC"), "", "PL.PLANDOSE", "P.E"),
            &[PL, "columns.X.derivation.source"],
            KEY_TYPES,
        ),
        // Under row templates.
        (
            record("", "      X: P.D\n", X, C).edit(
                "intermediates:\n",
                &format!("intermediates:\n{}", lookup("D: \"DM.RFSTDTC + 1\"")),
            ),
            &[at],
            OPERAND,
        ),
        // Derivations that bind are read for the row (LOOKUP-15).
        (
            implicit(&lookup("E: DM.RFSTDTC"), "", X, "P.E"),
            &["intermediates.P.derivations.E.source"],
            current,
        ),
        (
            record("", "      C: P.E\n", X, C).edit(
                "intermediates:\n",
                &format!("intermediates:\n{}", lookup("E: DM.RFSTDTC")),
            ),
            &["intermediates.P.derivations.E.source"],
            current,
        ),
        (
            implicit(&unique("E: DM.RFSTDTC"), "", X, "P.E"),
            &[
                "intermediates.P.verifications[0].unique",
                "intermediates.P.derivations.E.source",
            ],
            "correlated_derivation_with_unique_verification Some(\"LOOKUP-17\")",
        ),
    ]);
}

const PATTERN: &str = "invalid_regex Some(\"TEXT-6\")";
const TEXT: &str = "incompatible_input_type Some(\"TEXT-1\")";

/// Binding binds a text extraction or replacement by its source: once the
/// source binds it types it by the reading scope's catalog, refusing a
/// source that is not text (TEXT-1), and once it takes the source it refuses
/// the pattern and group (TEXT-6, TEXT-10). Without row templates preflight
/// checks a source a declaration types; elsewhere `check()` follows binding,
/// in a row template and the column phase, a `case` result, a lookup's
/// derivation, an aggregate's derive binding and, under row templates, a
/// lookup's or an aggregate's key value. The catalog does not type a
/// lookup's field, so a source that reads one is taken as it binds.
#[test]
fn check_reports_a_text_pattern_where_binding_takes_its_source_as_domain_does() {
    let text = |derivation: &str| record("", &format!("      C: {derivation}\n"), X, C);
    let column = |c: &str| record("", "", X, c);
    let lookup = |lookups: &str, c: &str| implicit(lookups, "", X, c);
    let extract = |source: &str| {
        format!("{{str_extract: {{source: {source}, pattern: \"^(?=A)\", group: 1}}}}")
    };
    let keyed = |key: &str, derivations: &str| {
        record("", derivations, X, C).edit(
            "intermediates:\n",
            &format!("intermediates:\n  - {{id: Q, dataset: PLAN, key: {{USUBJID: {key}}}}}\n"),
        )
    };
    assert_cases(&[
        // #3585's report: the column phase, a row template and a replacement,
        // with a Python-only named group and a lookahead.
        (
            column("{str_extract: {source: USUBJID, pattern: \"^(?P<site>[A-Z]+)-\", group: 1}}"),
            &["columns.C.derivation.str_extract.pattern"],
            PATTERN,
        ),
        (
            text("{str_extract: {source: VS.USUBJID, pattern: \"^(?P<site>[A-Z]+)-\", group: 1}}"),
            &["rows.record.derivations.C.str_extract.pattern"],
            PATTERN,
        ),
        (
            text("{str_extract: {source: USUBJID, pattern: \"^([A-Z]+)(?=-)\", group: 1}}"),
            &["rows.record.derivations.C.str_extract.pattern"],
            PATTERN,
        ),
        (
            column("{str_replace: {source: USUBJID, pattern: \"-(?P<n>[0-9]+)$\", replacement: \"\"}}"),
            &["columns.C.derivation.str_replace.pattern"],
            PATTERN,
        ),
        (
            text("{str_extract: {source: VS.VSTESTCD, pattern: \"^(A)\", group: 2}}"),
            &["rows.record.derivations.C.str_extract.group"],
            "regex_group_out_of_range Some(\"TEXT-10\")",
        ),
        (
            text(&format!(
                "{{case: [{{when: \"VS.SYSBP > 1\", then: {}}}, {{otherwise: \"'B'\"}}]}}",
                extract("VS.USUBJID")
            )),
            &["rows.record.derivations.C.case[0].then.str_extract.pattern"],
            PATTERN,
        ),
        // A source that is not text, whose pattern binding then leaves.
        (
            text(&extract("VS.VSDY")),
            &["rows.record.derivations.C.str_extract.source"],
            TEXT,
        ),
        (
            column("{str_replace: {source: VSDY, pattern: \"A\", replacement: \"B\"}}"),
            &["columns.C.derivation.str_replace.source"],
            TEXT,
        ),
        // A lookup's field, without and with row templates.
        (
            lookup("", &extract("PB.DOSENUM")),
            &["columns.C.derivation.str_extract.pattern"],
            PATTERN,
        ),
        (
            text(&extract("PB.DOSENUM")),
            &["rows.record.derivations.C.str_extract.pattern"],
            PATTERN,
        ),
        // Over a lookup's records and an aggregate's.
        (
            lookup(
                &format!(
                    "  - {{id: P, dataset: PLAN, key: [USUBJID], derivations: {{D: {}}}}}\n",
                    extract("USUBJID")
                ),
                "P.D",
            ),
            &["intermediates.P.derivations.D.str_extract.pattern"],
            PATTERN,
        ),
        (
            lookup(
                "  - {id: P, dataset: PLAN, key: [USUBJID], derivations: {D: {str_extract: {source: DOSENUM, pattern: \"^(A)\", group: 1}}}}\n",
                "P.D",
            ),
            &["intermediates.P.derivations.D.str_extract.source"],
            TEXT,
        ),
        (
            lookup(
                &format!(
                    "  - {{id: Q, dataset: PLAN, key: {{USUBJID: HOME.USUBJID}}}}\n\
                     \x20 - {{id: P, dataset: HOME, key: [USUBJID], derivations: {{D: {}}}}}\n",
                    extract("Q.DOSENUM")
                ),
                "P.D",
            ),
            &["intermediates.P.derivations.D.str_extract.pattern"],
            PATTERN,
        ),
        (
            lookup(
                "",
                &format!(
                    "{{aggregate: {{expr: \"COUNT(F)\", derivations: {{F: {{type: str, derivation: {}}}}}}}}}",
                    extract("PLAN.USUBJID")
                ),
            ),
            &["columns.C.derivation.aggregate.derivations.F.derivation.str_extract.pattern"],
            PATTERN,
        ),
        // A key value under row templates; binding binds nothing more of
        // its lookup.
        (
            keyed(&extract("VS.USUBJID"), "      X: Q.PLANDOSE\n"),
            &[
                "intermediates.Q.key.USUBJID.str_extract.pattern",
                "rows.record.derivations.X.source",
            ],
            PATTERN,
        ),
        (
            keyed("{str_template: \"{VS.VSDY}-{VS.USUBJID}\"}", "      X: Q.PLANDOSE\n"),
            &[
                "intermediates.Q.key.USUBJID.str_template.template",
                "rows.record.derivations.X.source",
            ],
            TEXT,
        ),
        (
            record(
                "",
                "",
                &format!(
                    "{{aggregate: {{expr: \"SUM(PLAN.PLANDOSE)\", key: {{USUBJID: {}}}}}}}",
                    extract("USUBJID")
                ),
                C,
            ),
            &["columns.X.derivation.aggregate.key.USUBJID.str_extract.pattern"],
            PATTERN,
        ),
        // The 0.5 surface, #3585's own specification.
        (
            "schema_version: \"1.0\"\ndomain: ADTOY\nkeys: [USUBJID]\n\
             input: {SRC: vs.csv}\n\
             output: {path: out.csv, columns: [USUBJID, SITE]}\n\
             columns:\n\
             \x20 - {name: USUBJID, type: str, label: Subject}\n\
             \x20 - name: SITE\n\
             \x20   type: str\n\
             \x20   label: Site\n\
             \x20   derivation:\n\
             \x20     str_extract: {source: USUBJID, pattern: '^(?P<site>[A-Z]+)-', group: 1}\n\
             rows:\n\
             \x20 - id: subjects\n\
             \x20   dataset: SRC\n\
             \x20   derivations:\n\
             \x20     USUBJID: SRC.USUBJID\n"
                .into(),
            &["columns.SITE.derivation.str_extract.pattern"],
            PATTERN,
        ),
    ]);
    // A valid pattern over a text source.
    assert_clean(&[
        text("{str_extract: {source: VS.USUBJID, pattern: \"^([A-Z]+)-\", group: 1}}"),
        column("{str_replace: {source: USUBJID, pattern: \"-([0-9]+)$\", replacement: \"\"}}"),
        lookup(
            "  - {id: P, dataset: PLAN, key: [USUBJID], derivations: {D: {str_extract: {source: USUBJID, pattern: \"^([A-Z]+)\", group: 1}}}}\n",
            "P.D",
        ),
    ]);
}

/// One row template over VS grouped by subject, whose row phase derives X.
fn grouped(x: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADVS\nkeys: [STUDYID, USUBJID]\n{}\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, X]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str}}\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: X, type: float}}\n\
         rows:\n\
         \x20 - id: subject\n    dataset: VS\n    group_by: [VS.STUDYID, VS.USUBJID]\n    derivations:\n\
         \x20     STUDYID: VS.STUDYID\n      USUBJID: VS.USUBJID\n      X: {x}\n",
        declarations("")
    )
}

/// A qualified aggregate's filter reads only fields of the relation it
/// reduces, and the filter of one that reduces the output rows only output
/// columns (`aggregates::bind_filter`): binding refuses each other name as
/// unknown (NAME-3) at the filter, once, in written order, after the
/// aggregate's header and before the key it matches on, with or without
/// `rows`. Whether the relation stores a field of its own is left to its
/// records.
#[test]
fn check_reports_an_aggregate_filter_s_read_outside_what_it_reduces_as_domain_does() {
    let aggregate = |expr: &str, filter: &str| {
        format!("{{aggregate: {{expr: \"{expr}\", filter: \"{filter}\"}}}}")
    };
    let plan = |filter: &str| implicit("", "", &aggregate("SUM(PLAN.PLANDOSE)", filter), C);
    let at = "columns.X.derivation.aggregate.filter";
    let unknown = |path: &str, name: &str| {
        format!(
            "unknown_field Some(\"NAME-3\") [\"{path}\"] {{\"identifier\": String(\"{name}\")}}"
        )
    };
    let first = [
        unknown(at, "XX.Y"),
        unknown(at, "DM.AGE"),
        unknown(at, "VSDY"),
        unknown(at, "PB.PLANDOSE"),
        unknown(at, "VS.SYSBP"),
        unknown(at, "HOME.SYSBP"),
        unknown(at, "F"),
        unknown("rows.subject.derivations.X.aggregate.filter", "DM.AGE"),
    ];
    assert_cases(&[
        // #3629's three over PLAN without rows: a qualifier nothing
        // declares, another input and a bare name.
        (plan("XX.Y > 1"), &[at], &first[0]),
        (plan("DM.AGE > 1"), &[at], &first[1]),
        (plan("VSDY > 1"), &[at], &first[2]),
        // Each name once, in written order, beside a field of PLAN.
        (
            plan("PLAN.DOSENUM > 1 AND XX.Y > 1 AND XX.Y < 3 AND VSDY > 0"),
            &[at, at],
            &first[0],
        ),
        // A lookup's field, and the driver's.
        (plan("PB.PLANDOSE > 1"), &[at], &first[3]),
        (plan("VS.SYSBP > 1"), &[at], &first[4]),
        // An aggregate over the driver, matching on a key of its own.
        (
            implicit(
                "",
                "",
                "{aggregate: {expr: \"SUM(VS.SYSBP)\", key: [USUBJID], filter: \"HOME.SYSBP > 1\"}}",
                C,
            ),
            &[at],
            &first[5],
        ),
        // A derive binding is no field of the relation.
        (
            implicit(
                "",
                "",
                "{aggregate: {expr: \"COUNT(F)\", derivations: {F: {type: str, derivation: PLAN.USUBJID}}, filter: \"F = 'x'\"}}",
                C,
            ),
            &[at],
            &first[6],
        ),
        // An aggregate that reduces the output rows reads output columns.
        (
            implicit(
                "",
                "",
                "{aggregate: {expr: \"SUM(VSDY)\", group_by: [USUBJID], filter: \"DM.AGE > 1 AND NOPE > 2 AND VSDY > 3\"}}",
                C,
            ),
            &[at, at],
            &first[1],
        ),
        // Before HOME's missing key, which the aggregate would match on
        // (LOOKUP-5).
        (
            implicit("", "", &aggregate("SUM(HOME.SYSBP)", "XX.Y > 1"), C),
            &[at, "columns.X.derivation.aggregate"],
            &first[0],
        ),
        // Under row templates: the column phase, and a grouped template's
        // row phase, whose aggregate reduces its driver.
        (
            record("", "", &aggregate("SUM(PLAN.PLANDOSE)", "XX.Y > 1"), C),
            &[at],
            &first[0],
        ),
        (
            grouped(&aggregate("SUM(VS.SYSBP)", "DM.AGE > 1")),
            &["rows.subject.derivations.X.aggregate.filter"],
            &first[7],
        ),
    ]);
    // The same filters reading what each reduces bind.
    assert_clean(&[
        plan("PLAN.DOSENUM > 1"),
        implicit(
            "",
            "",
            "{aggregate: {expr: \"SUM(VSDY)\", group_by: [USUBJID], filter: \"VSDY > 3\"}}",
            C,
        ),
        grouped(&aggregate("SUM(VS.SYSBP)", "VS.VSDY > 1")),
    ]);
    // A field PLAN may or may not store is left to its records.
    assert_subsequence(&[(plan("PLAN.NOPE > 1"), &[], &[at])]);
}

/// A lookup filter's field qualified by nothing declared, as both routes
/// render it, suggesting the donor's same-named field when it has one.
fn undeclared_qualifier(intermediate: &str, identifier: &str, suggestion: Option<&str>) -> String {
    let suggestion = suggestion.map_or_else(String::new, |field| {
        format!(", \"suggestion\": String(\"{field}\")")
    });
    format!(
        "unknown_field Some(\"LOOKUP-7\") [\"intermediates.{intermediate}.filter\"] \
         {{\"identifier\": String(\"{identifier}\"), \"intermediate\": String(\"{intermediate}\"){suggestion}}}"
    )
}

/// A lookup's filter reads its donor's records and another input's current
/// record: a field qualified by nothing declared, no input, lookup or the
/// lookup's own dataset, binds nothing whatever any input stores, so binding
/// refuses it as unknown (LOOKUP-7) at the filter, once, in written order,
/// suggesting the donor's same-named field when the donor record has one,
/// and binds nothing of the lookup, whose every read is unknown (NAME-3).
/// `check` reports it where the declarations decide that suggestion (D220):
/// a field the lookup derives, a field the input's `types` names, a
/// generated ordinal, a linked producer's output columns, and over SELF the
/// columns every row template derives. Where only the donor's records decide
/// it, `check` leaves the case to `domain`.
#[test]
fn check_reports_a_lookup_filter_s_undeclared_qualifier_where_its_suggestion_is_declared() {
    let lookup = |fields: &str| {
        implicit(
            &format!("  - {{id: P, dataset: PLAN, key: [USUBJID]{fields}}}\n"),
            "",
            "P.PLANDOSE",
            C,
        )
    };
    let filtered = |filter: &str| lookup(&format!(", filter: \"{filter}\""));
    let unique = |filter: &str| {
        lookup(&format!(
            ", filter: \"{filter}\", verifications: [{{unique: [USUBJID]}}]"
        ))
    };
    let selves = |filter: &str| {
        record("", "", X, "S.USUBJID").edit(
            "intermediates:\n",
            &format!(
                "intermediates:\n  - {{id: S, dataset: SELF, key: [USUBJID], filter: \"{filter}\"}}\n"
            ),
        )
    };
    let linked = |filter: &str| {
        implicit(
            &format!("  - {{id: A, dataset: ADSL, key: [USUBJID], filter: \"{filter}\"}}\n"),
            "",
            X,
            "A.SEX",
        )
        .edit(
            "  DM: {path: dm.csv,",
            "  ADSL: {path: adsl.csv, schema: spec_adsl.yaml, key: [USUBJID]}\n  DM: {path: dm.csv,",
        )
    };
    let at = "intermediates.P.filter";
    let read = "columns.X.derivation.source";
    let first = [
        undeclared_qualifier("P", "XX.DOSENUM", Some("PLAN.DOSENUM")),
        undeclared_qualifier("P", "SELF.DOSENUM", Some("PLAN.DOSENUM")),
        undeclared_qualifier("P", "XX.D", Some("PLAN.D")),
        undeclared_qualifier("P", "XX.SEQ", Some("PLAN.SEQ")),
        undeclared_qualifier("S", "XX.VSDY", Some("SELF.VSDY")),
        undeclared_qualifier("S", "XX.X", None),
        undeclared_qualifier("A", "XX.AGE", Some("ADSL.AGE")),
        undeclared_qualifier("A", "XX.NOPE", None),
    ];
    assert_cases(&[
        // PLAN's `types` names DOSENUM; SELF qualifies nothing declared for
        // a lookup over an input.
        (filtered("XX.DOSENUM > 1"), &[at, read], &first[0]),
        (filtered("SELF.DOSENUM > 1"), &[at, read], &first[1]),
        // A field the lookup derives, and a generated ordinal.
        (
            lookup(", derivations: {D: \"PLAN.DOSENUM + 1\"}, filter: \"XX.D > 1\""),
            &[at, read],
            &first[2],
        ),
        (
            filtered("XX.SEQ > 1").edit(
                "PLAN: {path: plan.csv, key: [USUBJID],",
                "PLAN: {path: plan.csv, key: [USUBJID], ordinal: SEQ,",
            ),
            &[at, read],
            &first[3],
        ),
        // Each name once; binding admits no correlated read of a lookup it
        // does not bind, nor that read's correlation with its uniqueness
        // verification (LOOKUP-17).
        (
            filtered("XX.DOSENUM > 1 AND DM.AGE > 1 AND XX.DOSENUM < 3"),
            &[at, read],
            &first[0],
        ),
        (
            unique("XX.DOSENUM > 1 AND DM.AGE > 1"),
            &[at, read],
            &first[0],
        ),
        // After the lookup's key types fail.
        (
            implicit("", "", "PL.PLANDOSE", C).edit(
                "key: {DOSENUM: VS.USUBJID}}",
                "key: {DOSENUM: VS.USUBJID}, filter: \"XX.DOSENUM > 1\"}",
            ),
            &[PL, "intermediates.PL.filter", read],
            KEY_TYPES,
        ),
        // A row template's read of the lookup.
        (
            record("", "      X: P.PLANDOSE\n", X, C).edit(
                "intermediates:\n",
                "intermediates:\n  - {id: P, dataset: PLAN, key: [USUBJID], filter: \"XX.DOSENUM > 1\"}\n",
            ),
            &[at, "rows.record.derivations.X.source"],
            &first[0],
        ),
        // Over SELF, a column every row template derives is the donor's,
        // and a column the column phase derives is not.
        (
            selves("XX.VSDY > 1"),
            &["intermediates.S.filter", "columns.C.derivation.source"],
            &first[4],
        ),
        (
            selves("XX.X > 1"),
            &["intermediates.S.filter", "columns.C.derivation.source"],
            &first[5],
        ),
        // A linked producer's output columns decide either way.
        (
            linked("XX.AGE > 1"),
            &["intermediates.A.filter", "columns.C.derivation.source"],
            &first[6],
        ),
        (
            linked("XX.NOPE > 1"),
            &["intermediates.A.filter", "columns.C.derivation.source"],
            &first[7],
        ),
    ]);
    // A field PLAN's `types` does not name may or may not be stored, so
    // `check` leaves its suggestion and the lookup's refusal to `domain`
    // (D220), and reports a decided one beside it.
    assert_subsequence(&[
        (filtered("XX.Y > 1"), &[], &[at, read]),
        (unique("XX.Y > 1 AND DM.AGE > 1"), &[], &[at, read]),
        (
            filtered("XX.DOSENUM > 1 AND XX.Y > 1"),
            &[at, read],
            &[at, at, read],
        ),
        (
            filtered("XX.Y > 1 AND XX.DOSENUM > 1"),
            &[at, read],
            &[at, at, read],
        ),
    ]);
}
