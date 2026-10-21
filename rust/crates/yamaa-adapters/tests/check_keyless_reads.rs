#![cfg(feature = "yaml")]
//! `check` reports a read of a 1.0 input that declares no `key` as `domain`
//! does, before any input is read (#3307, D111, P9): an implicit join, or a
//! qualified aggregate without `key`, has nothing to match on and fails at
//! the read as `no_applicable_keys` (LOOKUP-5). Every case lists the same
//! issues in both routes and hosts, in binding's order, which binds each
//! row template before the column phase and the column phase once for each
//! driver's rows; and a column a `copy` entry copies from such an input is
//! such a read (D164).
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::{diagnostic::Diagnostic, domain_entry::Request, project_function::Language};

/// Rendered issues, each with its spec paths.
type Issues = Vec<(String, Vec<String>)>;

struct Fixture(PathBuf);
impl Fixture {
    fn new(specification: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "yamaa-keyless-reads-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("spec.yaml"), specification).unwrap();
        for (name, content) in [
            (
                "vs.csv",
                "STUDYID,USUBJID,VSTESTCD,VSDTC,VSDY,SYSBP,DIABP\nS,S-1,SYSBP,2025-01-01,1,120,80\n",
            ),
            (
                "home.csv",
                "STUDYID,USUBJID,VSTESTCD,VSDTC,VSDY,SYSBP,DIABP\nS,S-1,DIABP,2025-01-03,3,119,79\n",
            ),
            (
                "plan.csv",
                "STUDYID,USUBJID,DOSENUM,PLANDOSE\nS,S-1,1,10\n",
            ),
            ("dm.csv", "STUDYID,USUBJID,RFSTDTC,AGE\nS,S-1,2025-01-01,40\n"),
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
            ("ae.csv", "STUDYID,USUBJID,AESEQ\nS,S-1,1\n"),
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
            example: "keyless-reads",
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
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

/// A read of the keyless input `dataset` at `path`, by a qualified aggregate
/// when `aggregate`, as both routes render it.
fn keyless(path: &str, dataset: &str, aggregate: bool) -> String {
    format!(
        "no_applicable_keys Some(\"LOOKUP-5\") [\"{path}\"] {{\"dataset\": String(\"{dataset}\"), \
         \"hint\": String(\"declare `key` on the input{}\")}}",
        if aggregate {
            " or on the aggregate"
        } else {
            ""
        }
    )
}

/// A specification, the spec paths of its issues in order, and its first
/// issue, or how it starts.
type Case<'a> = (String, &'a [&'a str], String);

/// A case whose first issue is a join read of the keyless `dataset`.
fn read<'a>(specification: String, paths: &'a [&'a str], dataset: &str) -> Case<'a> {
    let first = keyless(paths[0], dataset, false);
    (specification, paths, first)
}

/// A case whose first issue is a qualified aggregate over the keyless
/// `dataset`.
fn reduced<'a>(specification: String, paths: &'a [&'a str], dataset: &str) -> Case<'a> {
    let first = keyless(paths[0], dataset, true);
    (specification, paths, first)
}

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
                    .is_some_and(|(issue, _)| issue.starts_with(first.as_str())),
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

/// The inputs: VS and HOME declare `vs` and `home` as their key, DM `dm`,
/// each `""` for none, and PLAN `[USUBJID]`.
fn inputs(vs: &str, home: &str, dm: &str) -> String {
    format!(
        "input:\n\
         \x20 VS: {{path: vs.csv, {vs}types: {{VSDTC: date, VSDY: int, SYSBP: float, DIABP: float}}}}\n\
         \x20 HOME: {{path: home.csv, {home}types: {{VSDTC: date, VSDY: int, SYSBP: float, DIABP: float}}}}\n\
         \x20 PLAN: {{path: plan.csv, key: [USUBJID], types: {{DOSENUM: int, PLANDOSE: float}}}}\n\
         \x20 DM: {{path: dm.csv, {dm}types: {{RFSTDTC: date, AGE: int}}}}\n"
    )
}

/// A key declaration on an input, written in its flow mapping.
const KEY: &str = "key: [USUBJID], ";

/// No rows: VS is the base, DM declares `dm` as its key, and the float X,
/// the str C and the int N derive `x`, `c` and `n`. `extra` holds more
/// columns before them.
fn implicit(dm: &str, extra: &str, x: &str, c: &str, n: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADVS\nkeys: [STUDYID, USUBJID, VSDY]\n{}base: VS\n\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, VSDY, X, C, N]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str, derivation: VS.STUDYID}}\n\
         \x20 - {{name: USUBJID, type: str, derivation: VS.USUBJID}}\n\
         \x20 - {{name: VSDY, type: int, derivation: VS.VSDY}}\n{extra}\
         \x20 - {{name: X, type: float, derivation: {x}}}\n\
         \x20 - {{name: C, type: str, derivation: {c}}}\n\
         \x20 - {{name: N, type: int, derivation: {n}}}\n",
        inputs("", "", dm)
    )
}

/// One row template over VS, a row per record, with `filter` and the
/// derivations `derivations` besides its own; DM declares `dm` as its key,
/// and the float X, the str C and the int N default to `x`, `c` and `n`.
fn record(dm: &str, filter: &str, derivations: &str, x: &str, c: &str, n: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADVS\nkeys: [STUDYID, USUBJID, PARAMCD, VSDY]\n{}\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, PARAMCD, VSDY, AVAL, X, C, N]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str}}\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: PARAMCD, type: str}}\n\
         \x20 - {{name: VSDY, type: int}}\n\
         \x20 - {{name: AVAL, type: float}}\n\
         \x20 - {{name: X, type: float, derivation: {x}}}\n\
         \x20 - {{name: C, type: str, derivation: {c}}}\n\
         \x20 - {{name: N, type: int, derivation: {n}}}\n\
         rows:\n\
         \x20 - id: record\n    dataset: VS\n{filter}    derivations:\n\
         \x20     STUDYID: VS.STUDYID\n      USUBJID: VS.USUBJID\n      PARAMCD: \"'SYSBP'\"\n\
         \x20     VSDY: VS.VSDY\n      AVAL: VS.SYSBP\n{derivations}",
        inputs("", "", dm)
    )
}

/// Row templates over VS and HOME, which both declare `key` as their key;
/// X defaults to `x`, and `home` holds more of HOME's derivations.
fn two(key: &str, x: &str, home: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADVS\nkeys: [STUDYID, USUBJID, PARAMCD, VSDY]\n{}\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, PARAMCD, VSDY, AVAL, X]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str}}\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: PARAMCD, type: str}}\n\
         \x20 - {{name: VSDY, type: int}}\n\
         \x20 - {{name: AVAL, type: float}}\n\
         \x20 - {{name: X, type: float, derivation: {x}}}\n\
         rows:\n\
         \x20 - id: clinic\n    dataset: VS\n    derivations:\n\
         \x20     STUDYID: VS.STUDYID\n      USUBJID: VS.USUBJID\n      PARAMCD: \"'CLINIC'\"\n\
         \x20     VSDY: VS.VSDY\n      AVAL: VS.SYSBP\n\
         \x20 - id: home\n    dataset: HOME\n    derivations:\n\
         \x20     STUDYID: HOME.STUDYID\n      USUBJID: HOME.USUBJID\n      PARAMCD: \"'HOME'\"\n\
         \x20     VSDY: HOME.VSDY\n      AVAL: HOME.SYSBP\n{home}",
        inputs(key, key, "")
    )
}

/// One row template over DOSES, a lookup of PLAN that drives it and so
/// declares no key (ROW-2), while PLAN declares `plan` as its key; `dose`
/// holds more of the template's derivations, and X defaults to `x`.
fn dosed(plan: &str, dose: &str, x: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADVS\nkeys: [STUDYID, USUBJID, PARAMCD]\n\
         input:\n  PLAN: {{path: plan.csv, {plan}types: {{DOSENUM: int, PLANDOSE: float}}}}\n\
         intermediates:\n  - {{id: DOSES, dataset: PLAN, filter: \"PLAN.DOSENUM > 0\"}}\n\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, PARAMCD, AVAL, X]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str}}\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: PARAMCD, type: str}}\n\
         \x20 - {{name: AVAL, type: float}}\n\
         \x20 - {{name: X, type: float, derivation: {x}}}\n\
         rows:\n\
         \x20 - id: dose\n    dataset: DOSES\n    derivations:\n\
         \x20     STUDYID: DOSES.STUDYID\n      USUBJID: DOSES.USUBJID\n      PARAMCD: \"'DOSE'\"\n\
         \x20     AVAL: DOSES.PLANDOSE\n{dose}"
    )
}

/// ADAE over AE, which copies AGE and SEX from ADSL, linked to its producer
/// and declaring `key`; with `rows`, a row template over AE builds the rows.
fn copied(key: &str, rows: bool) -> String {
    let (base, derived, template) = if rows {
        (
            "",
            ["", "", ""],
            "rows:\n  - id: events\n    dataset: AE\n    derivations:\n\
             \x20     STUDYID: AE.STUDYID\n      USUBJID: AE.USUBJID\n      AESEQ: AE.AESEQ\n",
        )
    } else {
        (
            "base: AE\n",
            [
                ", derivation: AE.STUDYID",
                ", derivation: AE.USUBJID",
                ", derivation: AE.AESEQ",
            ],
            "",
        )
    };
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADAE\nkeys: [STUDYID, USUBJID, AESEQ]\n\
         input:\n  AE: {{path: ae.csv, types: {{AESEQ: int}}}}\n\
         \x20 ADSL: {{path: adsl.csv, schema: spec_adsl.yaml{key}}}\n{base}\
         output:\n  path: adae.csv\n  columns: [STUDYID, USUBJID, AESEQ, AGE, SEX]\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str{}}}\n\
         \x20 - {{name: USUBJID, type: str{}}}\n\
         \x20 - {{name: AESEQ, type: int{}}}\n\
         \x20 - copy: {{dataset: ADSL, columns: [AGE, SEX]}}\n{template}",
        derived[0], derived[1], derived[2]
    )
}

#[test]
fn check_reports_each_keyless_read_without_rows_as_domain_does() {
    let (x, c, n) = ("0.0", "\"'A'\"", "0");
    let case = |x, c, n| implicit("", "", x, c, n);
    assert_cases(&[
        // A name, a structured source after its filter's refusal (LOOKUP-8),
        // and a source of `first_available` or of a mapping.
        read(case("DM.AGE", c, n), &["columns.X.derivation.source"], "DM"),
        (
            case(
                "{source: {variable: DM.AGE, require_match: true, filter: \"VS.VSDY > 1\"}}",
                c,
                n,
            ),
            &[
                "columns.X.derivation.source.filter",
                "columns.X.derivation.source",
            ],
            "unknown_field Some(\"LOOKUP-8\")".into(),
        ),
        read(
            case("{first_available: {sources: [DM.AGE, VS.SYSBP]}}", c, n),
            &["columns.X.derivation.first_available.sources[0]"],
            "DM",
        ),
        read(
            case(x, "{mapping: {source: DM.USUBJID, dict: {\"1\": one}}}", n),
            &["columns.C.derivation.mapping.source"],
            "DM",
        ),
        // A verb's operands, each in written order, and a placeholder at the
        // template's text.
        read(
            case("{greatest: {sources: [VS.SYSBP, DM.AGE, HOME.SYSBP]}}", c, n),
            &[
                "columns.X.derivation.greatest.sources[1]",
                "columns.X.derivation.greatest.sources[2]",
            ],
            "DM",
        ),
        read(
            case(
                x,
                c,
                "{diff: {type: date, start: DM.RFSTDTC, end: VS.VSDTC, unit: day}}",
            ),
            &["columns.N.derivation.diff.start"],
            "DM",
        ),
        read(
            case("{round_half_away_from_zero: {source: DM.AGE, digits: 0}}", c, n),
            &["columns.X.derivation.round_half_away_from_zero.source"],
            "DM",
        ),
        read(
            case(x, "{str_template: \"{VS.USUBJID}-{DM.USUBJID}\"}", n),
            &["columns.C.derivation.str_template.template"],
            "DM",
        ),
        // A flag's or `case` predicate's read then names an unknown field
        // (FORMULA-9); each arm's predicate precedes its result, and a later
        // arm follows an earlier result.
        read(
            case(x, "{flag: \"DM.AGE > 1 AND VS.VSDY > 1\"}", n),
            &[
                "columns.C.derivation.flag.condition",
                "columns.C.derivation.flag.condition",
            ],
            "DM",
        ),
        read(
            case(
                "{case: [{when: \"DM.AGE > 1\", then: 1.0}, \
                 {when: \"VSDY > 1\", then: {source: {variable: VS.SYSBP, require_match: true}}}, \
                 {when: \"HOME.VSDY > 1\", then: DM.AGE}, {otherwise: 2.0}]}",
                c,
                n,
            ),
            &[
                "columns.X.derivation.case[0].when",
                "columns.X.derivation.case[0].when",
                "columns.X.derivation.case[1].then.source.require_match",
                "columns.X.derivation.case[2].when",
                "columns.X.derivation.case[2].when",
                "columns.X.derivation.case[2].then.source",
            ],
            "DM",
        ),
        // A qualified aggregate without `key`: over another input, over the
        // base, through its derive bindings, and after its filter's grammar.
        reduced(
            case(x, c, "{aggregate: \"COUNT(DM.AGE)\"}"),
            &["columns.N.derivation.aggregate"],
            "DM",
        ),
        reduced(
            case(x, c, "{aggregate: \"COUNT(VS.VSDY)\"}"),
            &["columns.N.derivation.aggregate"],
            "VS",
        ),
        reduced(
            case(
                x,
                c,
                "{aggregate: {expr: \"COUNT(D)\", derivations: {D: {type: int, derivation: DM.AGE}}}}",
            ),
            &["columns.N.derivation.aggregate"],
            "DM",
        ),
        (
            case(
                x,
                c,
                "{aggregate: {expr: \"COUNT(DM.AGE)\", filter: \"DM.AGE >\"}}",
            ),
            &[
                "columns.N.derivation.aggregate.filter",
                "columns.N.derivation.aggregate",
            ],
            "invalid_expression Some(\"FORMULA-1\")".into(),
        ),
        // Each column in declaration order, a key first.
        read(
            implicit(
                "",
                "",
                x,
                "{str_template: \"{DM.USUBJID}\"}",
                "{case: [{when: \"VSDY > 1\", then: {aggregate: \"COUNT(DM.AGE)\"}}, {otherwise: 0}]}",
            )
            .replace("derivation: VS.USUBJID", "derivation: DM.USUBJID"),
            &[
                "columns.USUBJID.derivation.source",
                "columns.C.derivation.str_template.template",
                "columns.N.derivation.case[0].then.aggregate",
            ],
            "DM",
        ),
    ]);
    // DM declares its key; the same reads join.
    let keyed = |x, c, n| implicit(KEY, "", x, c, n);
    assert_clean(&[
        keyed("DM.AGE", c, n),
        keyed("{greatest: {sources: [VS.SYSBP, DM.AGE]}}", c, n),
        keyed(
            x,
            "{flag: \"DM.AGE > 1\"}",
            "{aggregate: \"COUNT(DM.AGE)\"}",
        ),
        keyed(
            "{case: [{when: \"DM.AGE > 1\", then: DM.AGE}, {otherwise: 2.0}]}",
            "{str_template: \"{DM.USUBJID}\"}",
            n,
        ),
    ]);
}

#[test]
fn check_reports_each_keyless_read_under_row_templates_as_domain_does() {
    let (x, c, n) = ("0.0", "\"'A'\"", "0");
    let row = |derivations| record("", "", derivations, x, c, n);
    let column = |x, c, n| record("", "", "", x, c, n);
    assert_cases(&[
        // The row phase: a name, a structured source that keeps its join
        // options, a placeholder, a mapping, and the template's filter, each
        // name once.
        read(
            row("      X: DM.AGE\n"),
            &["rows.record.derivations.X.source"],
            "DM",
        ),
        read(
            row("      X: {source: {variable: DM.AGE, require_match: true, \
                 between: {value: VS.VSDY, lower: AGE}}}\n"),
            &["rows.record.derivations.X.source"],
            "DM",
        ),
        read(
            row("      C: {str_template: \"{DM.USUBJID}\"}\n"),
            &["rows.record.derivations.C.str_template.template"],
            "DM",
        ),
        read(
            row("      C: {mapping: {source: DM.USUBJID, dict: {\"1\": one}}}\n"),
            &["rows.record.derivations.C.mapping.source"],
            "DM",
        ),
        read(
            record(
                "",
                "    filter: \"DM.AGE > 1 AND HOME.VSDY > 1 AND DM.AGE < 9\"\n",
                "",
                x,
                c,
                n,
            ),
            &["rows.record.filter", "rows.record.filter"],
            "DM",
        ),
        // The column phase: a name, a flag, a `case` predicate, an aggregate
        // without `key`, and a placeholder.
        read(
            column("DM.AGE", c, n),
            &["columns.X.derivation.source"],
            "DM",
        ),
        read(
            column(x, "{flag: \"DM.AGE > 1\"}", n),
            &["columns.C.derivation.flag.condition"],
            "DM",
        ),
        read(
            column(
                "{case: [{when: \"DM.AGE > 1\", then: 1.0}, {otherwise: 2.0}]}",
                c,
                n,
            ),
            &["columns.X.derivation.case[0].when"],
            "DM",
        ),
        reduced(
            column(x, c, "{aggregate: \"COUNT(DM.AGE)\"}"),
            &["columns.N.derivation.aggregate"],
            "DM",
        ),
        read(
            column(x, "{str_template: \"{DM.USUBJID}\"}", n),
            &["columns.C.derivation.str_template.template"],
            "DM",
        ),
        // The template's derivations and filter come before the column
        // phase.
        read(
            record(
                "",
                "    filter: \"DM.AGE > 1\"\n",
                "      C: {str_template: \"{DM.USUBJID}\"}\n",
                "DM.AGE",
                c,
                n,
            ),
            &[
                "rows.record.derivations.C.str_template.template",
                "rows.record.filter",
                "columns.X.derivation.source",
            ],
            "DM",
        ),
        // The input behind a template's driver lookup, in the row phase and
        // in the column phase.
        read(
            dosed("", "      X: PLAN.PLANDOSE\n", "0.0"),
            &["rows.dose.derivations.X.source"],
            "PLAN",
        ),
        read(
            dosed("", "", "PLAN.PLANDOSE"),
            &["columns.X.derivation.source"],
            "PLAN",
        ),
        // Each template's driver is another input for the other's rows: in a
        // derivation, and in the column phase, which binds the whole
        // derivation for each driver's rows in turn.
        read(
            two("", "0.0", "      X: VS.SYSBP\n"),
            &["rows.home.derivations.X.source"],
            "VS",
        ),
        read(
            two("", "VS.SYSBP", ""),
            &["columns.X.derivation.source"],
            "VS",
        ),
        read(
            two("", "{greatest: {sources: [VS.SYSBP, HOME.SYSBP]}}", ""),
            &[
                "columns.X.derivation.greatest.sources[1]",
                "columns.X.derivation.greatest.sources[0]",
            ],
            "HOME",
        ),
        // VS's rows read HOME through its join, which keeps the options;
        // HOME's rows read their own driver, which refuses them (LOOKUP-24).
        read(
            two(
                "",
                "{source: {variable: HOME.SYSBP, require_match: true}}",
                "",
            ),
            &[
                "columns.X.derivation.source",
                "columns.X.derivation.source.require_match",
            ],
            "HOME",
        ),
    ]);
    // Declared keys join the same reads.
    assert_clean(&[
        record(
            KEY,
            "    filter: \"DM.AGE > 1\"\n",
            "      C: {str_template: \"{DM.USUBJID}\"}\n",
            "DM.AGE",
            c,
            "{aggregate: \"COUNT(DM.AGE)\"}",
        ),
        dosed(KEY, "      X: PLAN.PLANDOSE\n", "PLAN.PLANDOSE"),
        two(
            KEY,
            "{greatest: {sources: [VS.SYSBP, HOME.SYSBP]}}",
            "      X: VS.SYSBP\n",
        ),
    ]);
}

/// A row template's issues come before the column phase's, as binding binds
/// them, also for the refusals `check` reported before (#3229).
#[test]
fn check_lists_a_row_template_s_issues_before_the_column_phase_s() {
    let specification = format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADVS\nkeys: [STUDYID, USUBJID, PARAMCD]\n{}\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, PARAMCD, AVAL, X, C]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str}}\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: PARAMCD, type: str}}\n\
         \x20 - {{name: AVAL, type: float}}\n\
         \x20 - {{name: X, type: float, derivation: 0.0}}\n\
         \x20 - {{name: C, type: str, derivation: {{mapping: {{source: VS.VSTESTCD, dict: {{\"1\": one}}}}}}}}\n\
         rows:\n\
         \x20 - id: subject\n    dataset: VS\n    group_by: [VS.STUDYID, VS.USUBJID]\n    derivations:\n\
         \x20     STUDYID: VS.STUDYID\n      USUBJID: VS.USUBJID\n      PARAMCD: \"'MAXBP'\"\n\
         \x20     AVAL: {{aggregate: \"MAX(VS.SYSBP)\"}}\n      X: VS.SYSBP\n",
        inputs("", "", "")
    );
    assert_cases(&[(
        specification,
        &[
            "rows.subject.derivations.X.source",
            "columns.C.derivation.mapping.source",
        ],
        "ungrouped_driver_field Some(\"ROW-5\")".into(),
    )]);
}

/// A `copy` from an input that does not drive the rows is an implicit join
/// (D164): without the input's `key`, each copied column's read is refused,
/// without row templates and in the column phase under them.
#[test]
fn check_reports_a_copy_from_a_keyless_input_as_domain_does() {
    let paths: &[&str] = &[
        "columns.AGE.derivation.source",
        "columns.SEX.derivation.source",
    ];
    assert_cases(&[
        read(copied("", false), paths, "ADSL"),
        read(copied("", true), paths, "ADSL"),
    ]);
    let key = ", key: [STUDYID, USUBJID]";
    assert_clean(&[copied(key, false), copied(key, true)]);
}
