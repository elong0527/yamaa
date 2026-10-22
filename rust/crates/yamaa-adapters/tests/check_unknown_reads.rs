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
//! bind (FORMULA-12). Every case lists the same issues in both routes and
//! hosts, in the same order.
use std::fs;
use yamaa_adapters::{file_application, specification_report::Identity};
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
        let identity = || Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "unknown-reads",
            specification,
            base_directory: ".",
        };
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
