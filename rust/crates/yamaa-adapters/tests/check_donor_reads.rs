#![cfg(feature = "yaml")]
//! `check` reports what binding refuses in a lookup's own derivations and in
//! an aggregate's derive bindings as `domain` does (#3474, #2844 decision 3).
//! Both read the records of one relation, with no implicit join: a structured
//! source's `require_match` and `between` fail once its read binds (LOOKUP-24,
//! LOOKUP-9); a lookup's derivation reads its donor record, the current
//! record of an input and other lookups, and fails at a read of a derivation
//! not yet derived or a name nothing declares (LOOKUP-15, FORMULA-9 in a
//! predicate), at a window's read of another record (LOOKUP-15, LOOKUP-16),
//! at an aggregate (LOOKUP-18) and at a read of a lookup it cannot match
//! there (LOOKUP-3, LOOKUP-16); placeholders and mapping sources are typed
//! (TEXT-1, EXPR-11, EXPR-13). Each lookup's derivations come before its key
//! types, which come before its filter. Every case lists the same issues in
//! both routes and hosts, in the same order.
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
            "yamaa-donor-reads-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("spec.yaml"), specification).unwrap();
        for (name, content) in [
            (
                "vs.csv",
                "STUDYID,USUBJID,VSTESTCD,VSDTC,VSDY,SYSBP\nS,S-1,SYSBP,2025-01-01,1,120\n",
            ),
            (
                "plan.csv",
                "STUDYID,USUBJID,DOSENUM,PLANDOSE,FIRST,LAST,METHOD\nS,S-1,1,10,1,2,AUTO\n",
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
            example: "donor-reads",
            specification,
            base_directory: ".",
        };
        let render = |issues: &[Diagnostic]| {
            issues
                .iter()
                .map(|issue| {
                    (
                        format!(
                            "{} {} {:?} {:?} {:?}",
                            issue.phase(),
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

/// A specification, the spec paths of its issues in order, and how its
/// first issue starts: its phase, condition and rule.
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

const REQUIRED: &str = "validation prohibited_construct Some(\"LOOKUP-24\")";
const RANGE: &str = "validation prohibited_construct Some(\"LOOKUP-9\")";
const UNREAD: &str = "validation unknown_field Some(\"LOOKUP-15\")";
const UNKNOWN: &str = "validation unknown_field Some(\"FORMULA-9\")";
const TEXT: &str = "validation incompatible_input_type Some(\"TEXT-1\")";

/// A 1.0 specification over the base VS, whose lookups are `lookups`
/// besides P, a lookup of PLAN with the derivations `derivations`; the float
/// X and the str C derive `x` and `c`, and the int N derives `n`.
fn spec(lookups: &str, derivations: &str, x: &str, c: &str, n: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADVS\nkeys: [STUDYID, USUBJID, VSDY]\n\
         input:\n\
         \x20 VS: {{path: vs.csv, types: {{VSDTC: date, VSDY: int, SYSBP: float}}}}\n\
         \x20 PLAN: {{path: plan.csv, key: [STUDYID, USUBJID], types: {{DOSENUM: int, PLANDOSE: float, FIRST: int, LAST: int}}}}\n\
         base: VS\n\
         intermediates:\n{lookups}\
         \x20 - {{id: P, dataset: PLAN, key: [USUBJID], derivations: {{{derivations}}}}}\n\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, VSDY, X, C, N]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str, derivation: VS.STUDYID}}\n\
         \x20 - {{name: USUBJID, type: str, derivation: VS.USUBJID}}\n\
         \x20 - {{name: VSDY, type: int, derivation: VS.VSDY}}\n\
         \x20 - {{name: X, type: float, derivation: {x}}}\n\
         \x20 - {{name: C, type: str, derivation: {c}}}\n\
         \x20 - {{name: N, type: int, derivation: {n}}}\n"
    )
}

/// P's derivations `derivations`, read by X (a float D).
fn read(derivations: &str) -> String {
    spec("", derivations, "P.D", "\"'A'\"", "0")
}

/// P's derivations `derivations`, read by C (a str D).
fn text(derivations: &str) -> String {
    spec("", derivations, "0.0", "P.D", "0")
}

/// Q, a lookup of PLAN that P's derivations may read.
const Q: &str = "  - {id: Q, dataset: PLAN, key: [USUBJID]}\n";

#[test]
fn check_reports_a_lookup_derivation_s_join_options_as_domain_does() {
    let required = |derivation: &str| format!("intermediates.P.derivations.D.{derivation}");
    assert_cases(&[
        // A read of the donor record, of an input's current record, of an
        // earlier derivation and of another lookup binds, and refuses both
        // options; `require_match` first.
        (
            read("D: {source: {variable: PLAN.FIRST, require_match: true}}"),
            &[&required("source.require_match")],
            REQUIRED,
        ),
        (
            read("D: {source: {variable: PLAN.FIRST, between: {value: PLAN.LAST, lower: FIRST}}}"),
            &[&required("source.between")],
            RANGE,
        ),
        (
            read("D: {source: {variable: VS.VSDY, require_match: true, between: {value: PLAN.LAST, lower: FIRST}}}"),
            &[&required("source.require_match"), &required("source.between")],
            REQUIRED,
        ),
        (
            read("A: PLAN.FIRST, D: {source: {variable: A, require_match: true}}"),
            &[&required("source.require_match")],
            REQUIRED,
        ),
        (
            spec(
                Q,
                "D: {source: {variable: Q.FIRST, require_match: true}}",
                "P.D",
                "\"'A'\"",
                "0",
            ),
            &[&required("source.require_match")],
            REQUIRED,
        ),
        // A source of `first_available` and of a `case` result, and a typed
        // derivation, which then converts nothing.
        (
            read("D: {first_available: {sources: [{variable: PLAN.FIRST, require_match: true}, PLAN.LAST]}}"),
            &[&required("first_available.sources[0].require_match")],
            REQUIRED,
        ),
        (
            read("D: {case: [{when: \"PLAN.FIRST > 1\", then: {source: {variable: PLAN.LAST, require_match: true}}}, {otherwise: 2.0}]}"),
            &[&required("case[0].then.source.require_match")],
            REQUIRED,
        ),
        (
            read("D: {type: date, derivation: {source: {variable: PLAN.FIRST, require_match: true}}}")
                .replace("{name: X, type: float, derivation: P.D}", "{name: X, type: float, derivation: 0.0}")
                .replace("{name: N, type: int, derivation: 0}", "{name: N, type: int, derivation: P.D}"),
            &[&required("derivation.source.require_match")],
            REQUIRED,
        ),
        // A filter that reads another record, or filters one value, binds
        // nothing to refuse (LOOKUP-8).
        (
            read("D: {source: {variable: PLAN.FIRST, filter: \"VS.VSDY > 1\", require_match: true}}"),
            &[&required("source.filter")],
            "validation unknown_field Some(\"LOOKUP-8\")",
        ),
        (
            read("A: PLAN.FIRST, D: {source: {variable: A, filter: \"PLAN.FIRST > 1\", require_match: true}}"),
            &[&required("source.filter")],
            "validation prohibited_construct Some(\"LOOKUP-8\")",
        ),
    ]);
    // Without the options, or with a lookup's own, each read binds.
    assert_clean(&[
        read("A: PLAN.FIRST, D: {source: {variable: A, absent: 0.0}}"),
        spec(
            Q,
            "D: {first_available: {sources: [Q.FIRST, VS.VSDY, PLAN.LAST]}}",
            "P.D",
            "\"'A'\"",
            "0",
        ),
    ]);
}

#[test]
fn check_reports_each_read_a_lookup_derivation_cannot_make_as_domain_does() {
    let at = |derivation: &str| format!("intermediates.P.derivations.D.{derivation}");
    assert_cases(&[
        // A derivation not yet derived, bare or qualified, the derivation
        // itself, and a record nothing names: each read, in written order.
        (read("D: B, B: PLAN.FIRST"), &[&at("source")], UNREAD),
        (read("D: PLAN.B, B: PLAN.FIRST"), &[&at("source")], UNREAD),
        (read("D: D"), &[&at("source")], UNREAD),
        (read("D: XYZ.FIRST"), &[&at("source")], UNREAD),
        (read("D: SELF.FIRST"), &[&at("source")], UNREAD),
        (
            read("D: {greatest: {sources: [B, XYZ.A]}}, B: PLAN.FIRST"),
            &[&at("greatest.sources[0]"), &at("greatest.sources[1]")],
            UNREAD,
        ),
        (
            read("D: \"B + XYZ.A\", B: PLAN.FIRST"),
            &[
                "intermediates.P.derivations.D",
                "intermediates.P.derivations.D",
            ],
            UNREAD,
        ),
        (
            read("D: {source: {variable: B, require_match: true}}, B: PLAN.FIRST"),
            &[&at("source")],
            UNREAD,
        ),
        (
            text("D: {str_template: \"{B}-{XYZ.A}\"}, B: PLAN.METHOD"),
            &[&at("str_template.template"), &at("str_template.template")],
            UNREAD,
        ),
        // A predicate: a bare derivation not yet derived fails as a read
        // does, and a qualified name it cannot read is unknown to it, as is a
        // flag's read of a lookup.
        (
            read("D: {case: [{when: \"B > 1\", then: B}, {otherwise: 2.0}]}, B: PLAN.FIRST"),
            &[&at("case[0].when"), &at("case[0].then.source")],
            UNREAD,
        ),
        (
            read("D: {case: [{when: \"XYZ.A > 1 OR PLAN.B > 1\", then: 1.0}, {otherwise: 2.0}]}, B: PLAN.FIRST"),
            &[&at("case[0].when"), &at("case[0].when")],
            UNKNOWN,
        ),
        (
            text("D: {flag: \"B > 1\"}, B: PLAN.FIRST"),
            &[&at("flag.condition")],
            UNREAD,
        ),
        (
            spec(Q, "D: {flag: \"Q.FIRST > 1 OR XYZ.A > 1\"}", "0.0", "P.D", "0"),
            &[&at("flag.condition"), &at("flag.condition")],
            UNKNOWN,
        ),
        // A window reads the donor record alone: an input's current record,
        // a derivation not yet derived, then a lookup (LOOKUP-16).
        (
            spec(
                Q,
                "D: {row_value: {source: B, offset: 1, window: {order_by: [VS.VSDY, Q.FIRST]}}}, B: PLAN.FIRST",
                "P.D",
                "\"'A'\"",
                "0",
            ),
            &[
                &at("row_value.source"),
                &at("row_value.window.order_by[0]"),
                &at("row_value.window.order_by[1]"),
            ],
            UNREAD,
        ),
        // An aggregate has no context there (LOOKUP-18), after a reduction
        // of several relations.
        (
            read("D: {aggregate: \"COUNT(PLAN.FIRST)\"}"),
            &[&at("aggregate")],
            "validation invalid_aggregate_context Some(\"LOOKUP-18\")",
        ),
        (
            read("D: {aggregate: \"COUNT(PLAN.FIRST) + VSDY\"}"),
            &[&at("aggregate")],
            "validation mixed_relations Some(\"LOOKUP-18\")",
        ),
    ]);
    // Earlier derivations, the donor record, an input's current record and
    // another lookup, each as an operand, a predicate's name and a window's.
    assert_clean(&[
        spec(
            Q,
            "A: PLAN.FIRST, D: {case: [{when: \"A > 1 AND PLAN.A > 1 AND VS.VSDY > 1 AND Q.LAST > 1\", then: \"A + Q.FIRST\"}, {otherwise: PLAN.LAST}]}",
            "P.D",
            "\"'A'\"",
            "0",
        ),
        text("A: PLAN.FIRST, D: {flag: \"A > 1 AND PLAN.FIRST > 1 AND VS.VSDY > 1\"}"),
        read("A: PLAN.FIRST, D: {row_value: {source: A, offset: 1, window: {order_by: [PLAN.DOSENUM]}}}"),
    ]);
}

#[test]
fn check_types_a_lookup_derivation_s_placeholders_and_mapping_sources_as_domain_does() {
    let at = |derivation: &str| format!("intermediates.P.derivations.D.{derivation}");
    let template = at("str_template.template");
    let mapping = at("mapping.source");
    assert_cases(&[
        // The donor's typed field, an input's, an earlier derivation's result
        // and declared type, and another lookup's field.
        (text("D: {str_template: \"{PLAN.FIRST}\"}"), &[&template], TEXT),
        (text("D: {str_template: \"{VS.VSDY}\"}"), &[&template], TEXT),
        (
            text("A: {case: [{when: \"PLAN.FIRST > 1\", then: 1}, {otherwise: 2}]}, D: {str_template: \"{A}\"}"),
            &[&template],
            TEXT,
        ),
        (
            text("A: {type: int, derivation: PLAN.METHOD}, D: {str_template: \"{PLAN.A}\"}"),
            &[&template],
            TEXT,
        ),
        (
            spec(Q, "D: {str_template: \"{Q.FIRST}\"}", "0.0", "P.D", "0"),
            &[&template],
            TEXT,
        ),
        (
            text("D: {mapping: {source: PLAN.FIRST, dict: {\"1\": one}}}"),
            &[&mapping],
            "validation incompatible_input_type Some(\"EXPR-11\")",
        ),
        (
            spec(
                Q,
                "D: {mapping: {source: Q.FIRST, dict: {\"1\": one}}}",
                "0.0",
                "P.D",
                "0",
            ),
            &[&mapping],
            "validation incompatible_input_type Some(\"EXPR-11\")",
        ),
        (
            text("D: {cut: {source: VS.VSTESTCD, breaks: [1.0], labels: [a, b]}}"),
            &[&at("cut.source")],
            "validation incompatible_input_type Some(\"EXPR-13\")",
        ),
    ]);
    // A field the input leaves untyped is `str` (INPUT-5).
    assert_clean(&[text(
        "A: PLAN.METHOD, D: {str_template: \"{PLAN.METHOD}-{A}-{VS.VSTESTCD}\"}",
    )]);
}

/// A row template over DOSES, a lookup of PLAN that drives it and so declares
/// no key (ROW-2), beside one over VS; the lookups are `lookups`, and X reads
/// P's D.
fn driven(lookups: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADVS\nkeys: [STUDYID, USUBJID, PARAMCD]\n\
         input:\n\
         \x20 VS: {{path: vs.csv, types: {{VSDY: int, SYSBP: float}}}}\n\
         \x20 PLAN: {{path: plan.csv, key: [USUBJID], types: {{DOSENUM: int, PLANDOSE: float, FIRST: int}}}}\n\
         intermediates:\n  - {{id: DOSES, dataset: PLAN, filter: \"PLAN.DOSENUM > 0\"}}\n{lookups}\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, PARAMCD, AVAL, X]}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str}}\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: PARAMCD, type: str}}\n\
         \x20 - {{name: AVAL, type: float}}\n\
         \x20 - {{name: X, type: float, derivation: P.D}}\n\
         rows:\n\
         \x20 - id: vs\n    dataset: VS\n    derivations: {{STUDYID: VS.STUDYID, USUBJID: VS.USUBJID, PARAMCD: \"'V'\", AVAL: VS.SYSBP}}\n\
         \x20 - id: dose\n    dataset: DOSES\n    derivations: {{STUDYID: DOSES.STUDYID, USUBJID: DOSES.USUBJID, PARAMCD: \"'D'\", AVAL: DOSES.PLANDOSE}}\n"
    )
}

#[test]
fn check_reports_a_lookup_derivation_s_read_of_a_lookup_it_cannot_match_as_domain_does() {
    let at = "intermediates.P.derivations.D.source";
    assert_cases(&[
        // A 1.0 row driver declares no key (LOOKUP-3).
        (
            driven(
                "  - {id: P, dataset: PLAN, key: [USUBJID], derivations: {D: DOSES.PLANDOSE}}\n",
            ),
            &["intermediates.DOSES.key"],
            "validation missing_required_field Some(\"LOOKUP-3\")",
        ),
        // A key value read from another input's record has no value there
        // (LOOKUP-16), and no option is refused.
        (
            spec(
                "  - {id: Q, dataset: PLAN, key: {USUBJID: VS.USUBJID}}\n",
                "D: {source: {variable: Q.FIRST, require_match: true}}",
                "P.D",
                "\"'A'\"",
                "0",
            ),
            &[at],
            "validation unknown_field Some(\"LOOKUP-16\")",
        ),
        // A lookup over SELF completes after the lookups (LOOKUP-16).
        (
            driven(
                "  - {id: S, dataset: SELF, key: [USUBJID]}\n\
                 \x20 - {id: P, dataset: PLAN, key: [USUBJID], derivations: {D: S.AVAL}}\n",
            ),
            &[at],
            "validation phase_boundary Some(\"LOOKUP-16\")",
        ),
        // Binding binds nothing of a lookup whose key types fail, so each
        // read of it is unknown (LOOKUP-16), and unknown to a predicate
        // (FORMULA-9) (#3542).
        (
            spec(
                "  - {id: R, dataset: PLAN, key: {DOSENUM: VS.USUBJID}}\n",
                "D: R.PLANDOSE",
                "P.D",
                "\"'A'\"",
                "0",
            ),
            &["intermediates.R.key", at],
            "validation incompatible_input_type Some(\"LOOKUP-9\")",
        ),
        (
            spec(
                "  - {id: R, dataset: PLAN, key: {DOSENUM: VS.USUBJID}}\n",
                "D: {case: [{when: \"R.PLANDOSE > 1\", then: 1.0}, {otherwise: 2.0}]}",
                "P.D",
                "\"'A'\"",
                "0",
            ),
            &[
                "intermediates.R.key",
                "intermediates.P.derivations.D.case[0].when",
            ],
            "validation incompatible_input_type Some(\"LOOKUP-9\")",
        ),
    ]);
    // A key value read from the donor record matches there.
    assert_clean(&[spec(
        "  - {id: Q, dataset: PLAN, key: {DOSENUM: PLAN.FIRST}}\n",
        "D: {source: {variable: Q.FIRST, absent: 0.0}}",
        "P.D",
        "\"'A'\"",
        "0",
    )]);
}

#[test]
fn check_lists_a_lookup_s_issues_in_binding_s_order() {
    let required = "intermediates.P.derivations.D.source.require_match";
    let lookup = |fields: &str| {
        spec("", "", "P.D", "\"'A'\"", "0").replace(
            "  - {id: P, dataset: PLAN, key: [USUBJID], derivations: {}}\n",
            &format!("  - {{id: P, dataset: PLAN, {fields}}}\n"),
        )
    };
    assert_cases(&[
        // Its derivations, in written order, then its filter's grammar or
        // types; a column after the lookups.
        (
            lookup("key: [USUBJID], derivations: {D: {source: {variable: PLAN.FIRST, require_match: true}}, E: B, B: PLAN.LAST}"),
            &[required, "intermediates.P.derivations.E.source"],
            REQUIRED,
        ),
        (
            lookup("key: [USUBJID], filter: \"PLAN.FIRST >\", derivations: {D: {source: {variable: PLAN.FIRST, require_match: true}}}"),
            &[required, "intermediates.P.filter"],
            REQUIRED,
        ),
        (
            lookup("key: [USUBJID], filter: \"PLAN.FIRST > 'a'\", derivations: {D: {source: {variable: PLAN.FIRST, require_match: true}}}")
                .replace("derivation: VS.VSDY}", "derivation: {source: {variable: VS.VSDY, require_match: true}}}"),
            &[required, "intermediates.P.filter", "columns.VSDY.derivation.source.require_match"],
            REQUIRED,
        ),
    ]);
    // Its key types come between its derivations and its filter. Binding
    // binds nothing of a lookup whose key types fail and then names the
    // column's read of it unknown (#3542).
    assert_cases(&[(
        lookup("key: {DOSENUM: VS.USUBJID}, filter: \"PLAN.FIRST > 'a'\", derivations: {D: {source: {variable: PLAN.FIRST, require_match: true}}}"),
        &[
            required,
            "intermediates.P.key",
            "intermediates.P.filter",
            "columns.X.derivation.source",
        ],
        REQUIRED,
    )]);
}

#[test]
fn check_reports_what_binding_refuses_in_a_derive_binding_as_domain_does() {
    let aggregate = |derivations: &str| {
        spec("", "A: PLAN.FIRST", "0.0", "\"'A'\"", "0").replace(
            "{name: N, type: int, derivation: 0}",
            &format!("{{name: N, type: int, derivation: {{aggregate: {{expr: \"COUNT(F)\", derivations: {{{derivations}}}}}}}}}"),
        )
    };
    let at = |field: &str| format!("columns.N.derivation.aggregate.derivations.{field}");
    assert_cases(&[
        // A read of the relation and of an earlier binding refuses both
        // options.
        (
            aggregate("F: {type: float, derivation: {source: {variable: PLAN.PLANDOSE, require_match: true}}}"),
            &[&at("F.derivation.source.require_match")],
            REQUIRED,
        ),
        (
            aggregate("F: {type: float, derivation: {source: {variable: PLAN.PLANDOSE, between: {value: PLAN.LAST, lower: FIRST}}}}"),
            &[&at("F.derivation.source.between")],
            RANGE,
        ),
        (
            aggregate("G: {type: float, derivation: PLAN.PLANDOSE}, F: {type: float, derivation: {source: {variable: G, require_match: true}}}"),
            &[&at("F.derivation.source.require_match")],
            REQUIRED,
        ),
        // Placeholders and mapping sources are typed: the relation's field
        // and an earlier binding's declared type.
        (
            aggregate("F: {type: str, derivation: {str_template: \"{PLAN.FIRST}\"}}"),
            &[&at("F.derivation.str_template.template")],
            TEXT,
        ),
        (
            aggregate("G: {type: int, derivation: PLAN.FIRST}, F: {type: str, derivation: {str_template: \"{G}\"}}"),
            &[&at("F.derivation.str_template.template")],
            TEXT,
        ),
        (
            aggregate("F: {type: str, derivation: {mapping: {source: PLAN.FIRST, dict: {\"1\": one}}}}"),
            &[&at("F.derivation.mapping.source")],
            "validation incompatible_input_type Some(\"EXPR-11\")",
        ),
    ]);
    assert_clean(&[aggregate(
        "G: {type: float, derivation: PLAN.PLANDOSE}, F: {type: str, derivation: {str_template: \"{PLAN.METHOD}-{PLAN.STUDYID}\"}}",
    )]);
}

/// The two cases #3474 gives, on the 0.5 surface, where `PLAN` declares no
/// key and the lookup and the aggregate match on their own.
#[test]
fn check_reports_the_0_5_examples_as_domain_does() {
    let specification = |intermediates: &str, x: &str| {
        format!(
            "schema_version: \"1.0\"\ndomain: ADVS\nkeys: [STUDYID, USUBJID, VSDY]\n\
             input:\n\
             \x20 VS: {{path: vs.csv, types: {{VSDTC: date, VSDY: int, SYSBP: float}}}}\n\
             \x20 PLAN: {{path: plan.csv, types: {{DOSENUM: int, PLANDOSE: float, FIRST: int, LAST: int}}}}\n\
             base: VS\n{intermediates}\
             output: {{path: out.csv, columns: [STUDYID, USUBJID, VSDY, X]}}\n\
             columns:\n\
             \x20 - {{name: STUDYID, type: str, derivation: VS.STUDYID}}\n\
             \x20 - {{name: USUBJID, type: str, derivation: VS.USUBJID}}\n\
             \x20 - {{name: VSDY, type: int, derivation: VS.VSDY}}\n\
             \x20 - {{name: X, type: float, derivation: {x}}}\n"
        )
    };
    assert_cases(&[
        (
            specification(
                "intermediates:\n  - {id: PLANNED, dataset: PLAN, key: {DOSENUM: VS.VSDY}, no_match: null, derivations: {D: {source: {variable: PLAN.FIRST, require_match: true}}}}\n",
                "PLANNED.D",
            ),
            &["intermediates[0].derivations.D.source.require_match"],
            REQUIRED,
        ),
        (
            specification(
                "",
                "{aggregate: {expr: \"SUM(F)\", key: [STUDYID, USUBJID], derive: [{name: F, type: float, derivation: {source: {variable: PLAN.PLANDOSE, require_match: true}}}]}}",
            ),
            &["columns.X.derivation.aggregate.derive[0].derivation.source.require_match"],
            REQUIRED,
        ),
    ]);
}
