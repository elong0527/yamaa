#![cfg(feature = "yaml")]
//! One meaning per form of a `variable` field that names a current-row value,
//! on a 1.0 tree. In a row's read (#2934), in every `between.value` a bare
//! name is an output column alone (NAME-3), a driver-qualified name reads the
//! row's driver record, a name qualified by another input is refused and a
//! lookup-qualified name reads that lookup's column for the row (LOOKUP-9). A
//! key match value never reads a lookup (LOOKUP-5, LOOKUP-11), and a written
//! aggregate key reports its refusals at its `key` (LOOKUP-5). In a donor read
//! (#3414) a bare or donor-qualified name reads the donor record, a
//! lookup-qualified one that lookup's column for the donor record, and any
//! other record is refused at the reading derivation (LOOKUP-16). A range
//! value never reads the record of the intermediate that builds its row, and
//! a source binding's type refusal names its variable (LOOKUP-9). `check` reports
//! each refusal as `run` does, in `run`'s order and in both hosts, the later
//! read of a lookup its own key refused included; it leaves out only what the
//! declarations cannot decide, such as a field a relation does not store. A
//! 0.5 file keeps the rules at the `yamaa-v0.5.0` tag (D75).
use crate::support::scratch::Scratch;
use std::fs;
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_core::{
    diagnostic::Diagnostic, domain_entry::Request, json::Value, project_function::Language,
};

/// The run report's identity of every run here.
fn identity() -> Identity<'static> {
    Identity {
        runtime: "test",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: "variable-forms",
        specification: "spec.yaml",
        base_directory: ".",
    }
}

/// A specification and the inputs it reads, in a scratch directory.
struct Fixture(Scratch);
impl Fixture {
    fn new(specification: &str) -> Self {
        let scratch = Scratch::new("variable-forms");
        scratch.write("spec.yaml", specification);
        for (name, content) in INPUTS {
            scratch.write(name, content);
        }
        Self(scratch)
    }
    fn specification(&self) -> String {
        self.0.path().join("spec.yaml").to_str().unwrap().to_owned()
    }
    /// The issues of `check` and of `domain` in `host`, as the run report
    /// renders them.
    fn issues(&self, host: Language) -> (Vec<Value>, Vec<Value>) {
        let specification = self.specification();
        let request = || Request {
            specification: &specification,
            environment: None,
        };
        let rows = |issues: &[Diagnostic]| {
            issues
                .iter()
                .map(yamaa_adapters::report_storage::diagnostic_value)
                .map(|issue| {
                    let mut row = yamaa_core::json::Map::new();
                    for field in ["phase", "condition", "rule", "spec_paths", "context"] {
                        row.insert(field.into(), issue[field].clone());
                    }
                    Value::Object(row)
                })
                .collect::<Vec<_>>()
        };
        let checked = rows(&file_application::check(host, request(), identity()));
        let built = match file_application::domain(host, request(), identity()) {
            Ok(domain) => rows(&domain.issues()),
            Err(issues) => rows(&issues),
        };
        (checked, built)
    }
    /// The saved artifact of a run without issues.
    #[track_caller]
    fn saved(&self) -> String {
        let specification = self.specification();
        let mut domain = file_application::domain(
            Language::Python,
            Request {
                specification: &specification,
                environment: None,
            },
            identity(),
        )
        .unwrap();
        assert!(domain.issues().is_empty(), "{:?}", domain.issues());
        assert!(domain.save().unwrap());
        fs::read_to_string(self.0.path().join("out.csv")).unwrap()
    }
}

/// Clinic and home measurements of one subject, each with its day and the
/// device it was taken with; a reference day for the subject in REF and
/// another in OTHER; two day windows; the calibration days of each device;
/// two subjects' exposure records, one without a dose, and their periods.
const INPUTS: [(&str, &str); 9] = [
    (
        "clinic.csv",
        "STUDYID,USUBJID,SEQ,DAY,SID\nS,S-1,1,5,D1\nS,S-1,2,20,D2\n",
    ),
    (
        "home.csv",
        "STUDYID,USUBJID,SEQ,DAY,SID\nS,S-1,3,12,D1\nS,S-1,4,99,D9\n",
    ),
    ("ref.csv", "STUDYID,USUBJID,REFDAY,F\nS,S-1,12,D2\n"),
    (
        "win.csv",
        "STUDYID,NAME,LO,HI\nS,EARLY,1,10\nS,LATE,11,50\n",
    ),
    ("cal.csv", "STUDYID,SID,CALDY\nS,D1,1\nS,D1,2\nS,D2,3\n"),
    ("other.csv", "STUDYID,USUBJID,OTHDAY\nS,S-1,3\n"),
    (
        "subjects.csv",
        "STUDYID,USUBJID,EXSTDT\nS,S-1,2025-02-15\nS,S-2,2025-03-05\n",
    ),
    (
        "ex.csv",
        "STUDYID,USUBJID,EXSEQ,EXDOSE,EXSTDT\nS,S-1,1,0,2025-01-03\nS,S-1,2,50,2025-01-10\n\
         S,S-1,3,50,2025-02-01\nS,S-2,1,50,2025-03-01\n",
    ),
    (
        "periods.csv",
        "STUDYID,USUBJID,APERIOD,APERSDT,APEREDT\nS,S-1,1,2025-01-01,2025-01-31\n\
         S,S-1,2,2025-02-01,2025-02-28\nS,S-2,1,2025-03-01,\n",
    ),
];

/// A 1.0 specification: clinic and home templates over CLINIC and HOME, the
/// named intermediates `lookups` beside RK (REF's record of the subject), the
/// output column W deriving `column` and each template deriving `clinic` and
/// `home` for W (empty: none). ADAY is each row's day.
fn rows(lookups: &str, column: &str, clinic: &str, home: &str) -> String {
    let derived = |own: &str| {
        if own.is_empty() {
            String::new()
        } else {
            format!("      W: {own}\n")
        }
    };
    let column = if column.is_empty() {
        String::new()
    } else {
        format!(", derivation: {column}")
    };
    format!(
        "yamaa: \"1.0\"\n\
         type: spec\n\
         domain: OUT\n\
         keys: [STUDYID, USUBJID, SEQ]\n\
         input:\n\
         \x20 CLINIC: {{path: clinic.csv, types: {{SEQ: int, DAY: int}}}}\n\
         \x20 HOME: {{path: home.csv, types: {{SEQ: int, DAY: int}}}}\n\
         \x20 REF: {{path: ref.csv, types: {{REFDAY: int}}}}\n\
         \x20 WIN: {{path: win.csv, types: {{LO: int, HI: int}}, key: [STUDYID]}}\n\
         \x20 CAL: {{path: cal.csv, types: {{CALDY: int}}}}\n\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, SEQ, W]}}\n\
         intermediates:\n\
         \x20 - {{id: RK, dataset: REF, key: [STUDYID, USUBJID]}}\n\
         {lookups}\
         columns:\n\
         \x20 - {{name: STUDYID, type: str}}\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: SEQ, type: int}}\n\
         \x20 - {{name: ADAY, type: int}}\n\
         \x20 - {{name: W, type: str{column}}}\n\
         rows:\n\
         \x20 - id: clinic\n\
         \x20   dataset: CLINIC\n\
         \x20   derivations:\n\
         \x20     STUDYID: CLINIC.STUDYID\n\
         \x20     USUBJID: CLINIC.USUBJID\n\
         \x20     SEQ: CLINIC.SEQ\n\
         \x20     ADAY: CLINIC.DAY\n\
         {}\
         \x20 - id: home\n\
         \x20   dataset: HOME\n\
         \x20   derivations:\n\
         \x20     STUDYID: HOME.STUDYID\n\
         \x20     USUBJID: HOME.USUBJID\n\
         \x20     SEQ: HOME.SEQ\n\
         \x20     ADAY: HOME.DAY\n\
         {}",
        derived(clinic),
        derived(home)
    )
}

/// A 1.0 specification without row templates, built from CLINIC's records:
/// the named intermediates `lookups` beside RK, and W deriving `column`.
fn base(lookups: &str, column: &str) -> String {
    format!(
        "yamaa: \"1.0\"\n\
         type: spec\n\
         domain: OUT\n\
         keys: [STUDYID, USUBJID, SEQ]\n\
         input:\n\
         \x20 CLINIC: {{path: clinic.csv, types: {{SEQ: int, DAY: int}}}}\n\
         \x20 REF: {{path: ref.csv, types: {{REFDAY: int}}}}\n\
         \x20 WIN: {{path: win.csv, types: {{LO: int, HI: int}}, key: [STUDYID]}}\n\
         \x20 CAL: {{path: cal.csv, types: {{CALDY: int}}}}\n\
         base: CLINIC\n\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, SEQ, W]}}\n\
         intermediates:\n\
         \x20 - {{id: RK, dataset: REF, key: [STUDYID, USUBJID]}}\n\
         {lookups}\
         columns:\n\
         \x20 - {{name: STUDYID, type: str, derivation: CLINIC.STUDYID}}\n\
         \x20 - {{name: USUBJID, type: str, derivation: CLINIC.USUBJID}}\n\
         \x20 - {{name: SEQ, type: int, derivation: CLINIC.SEQ}}\n\
         \x20 - {{name: ADAY, type: int, derivation: CLINIC.DAY}}\n\
         \x20 - {{name: W, type: str, derivation: {column}}}\n"
    )
}

/// WI, a lookup of the day windows whose range value is `value`.
fn ranged(value: &str) -> String {
    format!("  - {{id: WI, dataset: WIN, key: [STUDYID], between: {{value: {value}, lower: LO, upper: HI}}}}\n")
}

/// WI, a lookup of the day windows that matches LO on `value`.
fn keyed(value: &str) -> String {
    format!("  - {{id: WI, dataset: WIN, key: {{STUDYID: STUDYID, LO: {value}}}}}\n")
}

/// An issue as the run report renders it.
fn issue(condition: &str, rule: &str, path: &str, context: Value) -> Value {
    yamaa_adapters::json!({
        "phase": "validation",
        "condition": condition,
        "rule": rule,
        "spec_paths": [path],
        "context": context,
    })
}

/// The read of a lookup whose own key refused it, which binding then names
/// unknown.
fn unknown_read(path: &str, identifier: &str) -> Value {
    issue(
        "unknown_field",
        "NAME-3",
        path,
        yamaa_adapters::json!({"identifier": identifier}),
    )
}

/// Run `specification` in both routes and hosts: `domain` reports `built`,
/// and `check` the same issues but the `left_out` ones, which only the
/// stored fields decide.
#[track_caller]
fn assert_issues(specification: &str, built: &[Value], left_out: &[Value]) {
    let fixture = Fixture::new(specification);
    for host in [Language::Python, Language::R] {
        let (checked, issues) = fixture.issues(host);
        assert_eq!(issues, built, "domain ({host:?})\n{specification}");
        let expected = built
            .iter()
            .filter(|issue| !left_out.contains(issue))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(checked, expected, "check ({host:?})\n{specification}");
    }
}

/// A bare `between.value` is an output column alone, so a field of the
/// driver's record that no column takes names nothing a row holds, at a
/// named intermediate's `between.value` in every template, whatever its
/// bounds bind; an aggregate's and a source binding's range already read it
/// so. The other forms keep their meaning.
#[test]
fn a_bare_range_value_reads_an_output_column_alone() {
    let refused = |path: &str, identifier: &str, owner: Option<(&str, &str)>| {
        let mut context = yamaa_core::json::Map::new();
        context.insert("identifier".into(), Value::String(identifier.into()));
        if let Some((key, name)) = owner {
            context.insert(key.into(), Value::String(name.into()));
        }
        issue("unknown_field", "LOOKUP-9", path, Value::Object(context))
    };
    let named = |identifier: &str| {
        refused(
            "intermediates.WI.between.value",
            identifier,
            Some(("intermediate", "WI")),
        )
    };
    // A named intermediate's range, read by both templates' rows, by the
    // clinic rows alone, and without row templates.
    for value in ["DAY", "NOPE"] {
        for specification in [
            rows(&ranged(value), "WI.NAME", "", ""),
            rows(&ranged(value), "", "WI.NAME", "\"'x'\""),
            base(&ranged(value), "WI.NAME"),
        ] {
            assert_issues(&specification, &[named(value)], &[]);
        }
    }
    // A bound the relation does not store fails first.
    assert_issues(
        &base(
            "  - {id: WI, dataset: WIN, key: [STUDYID], between: {value: DAY, lower: FROM}}\n",
            "WI.NAME",
        ),
        &[
            issue(
                "unknown_field",
                "LOOKUP-9",
                "intermediates.WI.between",
                yamaa_adapters::json!({"identifier": "FROM", "intermediate": "WI"}),
            ),
            named("DAY"),
        ],
        &[issue(
            "unknown_field",
            "LOOKUP-9",
            "intermediates.WI.between",
            yamaa_adapters::json!({"identifier": "FROM", "intermediate": "WI"}),
        )],
    );
    // A source binding's and an aggregate's range, in a row template, in the
    // column phase, in a case result and without row templates.
    let source = "{source: {variable: WIN.NAME, between: {value: DAY, lower: LO, upper: HI}}}";
    let aggregate =
        "{aggregate: {expr: 'MAX(WIN.NAME)', between: {value: DAY, lower: LO, upper: HI}}}";
    let joined = |path: &str| refused(path, "DAY", Some(("variable", "WIN.NAME")));
    for (specification, expected) in [
        (
            rows("", "", source, "\"'x'\""),
            joined("rows.clinic.derivations.W.source.between.value"),
        ),
        (
            rows("", source, "", ""),
            joined("columns.W.derivation.source.between.value"),
        ),
        (
            base(
                "",
                &format!("{{case: [{{when: 'ADAY > 0', then: {source}}}]}}"),
            ),
            joined("columns.W.derivation.case[0].then.source.between.value"),
        ),
        (
            base("", source),
            joined("columns.W.derivation.source.between.value"),
        ),
        (
            rows("", aggregate, "", ""),
            refused("columns.W.derivation.aggregate.between.value", "DAY", None),
        ),
        (
            base("", aggregate),
            refused("columns.W.derivation.aggregate.between.value", "DAY", None),
        ),
    ] {
        assert_issues(&specification, &[expected], &[]);
    }
    // An output column, the driver's qualified field and a lookup's column
    // keep their meaning: day 5 is in EARLY, days 12 and 20 in LATE, day 99
    // in none, and the subject's reference day 12 in LATE.
    for (specification, csv) in [
        (
            rows(&ranged("ADAY"), "WI.NAME", "", ""),
            "STUDYID,USUBJID,SEQ,W\nS,S-1,1,EARLY\nS,S-1,2,LATE\nS,S-1,3,LATE\nS,S-1,4,\n",
        ),
        (
            base(&ranged("CLINIC.DAY"), "WI.NAME"),
            "STUDYID,USUBJID,SEQ,W\nS,S-1,1,EARLY\nS,S-1,2,LATE\n",
        ),
        (
            rows(&ranged("RK.REFDAY"), "WI.NAME", "", ""),
            "STUDYID,USUBJID,SEQ,W\nS,S-1,1,LATE\nS,S-1,2,LATE\nS,S-1,3,LATE\nS,S-1,4,LATE\n",
        ),
    ] {
        let fixture = Fixture::new(&specification);
        for host in [Language::Python, Language::R] {
            assert_eq!(fixture.issues(host), (Vec::new(), Vec::new()), "{host:?}");
        }
        assert_eq!(fixture.saved(), csv, "{specification}");
    }
}

/// On the 0.5 surface a named intermediate's bare range value that no column
/// takes still reads the field of the driver's record, as at the
/// `yamaa-v0.5.0` tag (D75).
#[test]
fn a_0_5_bare_range_value_still_reads_the_driver_s_field() {
    let specification = "schema_version: \"1.0\"\n\
        domain: OUT\n\
        keys: [STUDYID, USUBJID, SEQ]\n\
        input:\n\
        \x20 CLINIC: {path: clinic.csv, types: {SEQ: int, DAY: int}}\n\
        \x20 WIN: {path: win.csv, types: {LO: int, HI: int}}\n\
        base: CLINIC\n\
        output: {path: out.csv, columns: [STUDYID, USUBJID, SEQ, W]}\n\
        intermediates:\n\
        \x20 - {id: WI, dataset: WIN, key: [STUDYID], between: {value: DAY, lower: LO, upper: HI}}\n\
        columns:\n\
        \x20 - {name: STUDYID, type: str, derivation: CLINIC.STUDYID}\n\
        \x20 - {name: USUBJID, type: str, derivation: CLINIC.USUBJID}\n\
        \x20 - {name: SEQ, type: int, derivation: CLINIC.SEQ}\n\
        \x20 - {name: W, type: str, derivation: WI.NAME}\n";
    assert_eq!(
        Fixture::new(specification).saved(),
        "STUDYID,USUBJID,SEQ,W\nS,S-1,1,EARLY\nS,S-1,2,LATE\n"
    );
}

/// A key match value reads what the current row holds: never a lookup's
/// column, a `SELF` field, a name nothing declares or a bare name that no
/// column takes (LOOKUP-5, LOOKUP-11). The pair refuses it, at `key` for a
/// name written alone and at `key.<column>` for any other value, and binding
/// binds nothing of the lookup, so its read fails too. A lookup another
/// lookup's derivations read
/// binds its key values for each reading context, so a row refuses it at its
/// read.
#[test]
fn a_key_match_value_never_reads_a_lookup() {
    let refused = |path: &str, identifier: &str| {
        issue(
            "unknown_field",
            "LOOKUP-5",
            path,
            yamaa_adapters::json!({"identifier": identifier, "intermediate": "WI"}),
        )
    };
    let read = unknown_read("rows.clinic.derivations.W.source", "WI.NAME");
    for (value, path, identifiers) in [
        ("RK.REFDAY", "intermediates.WI.key", &["RK.REFDAY"][..]),
        ("SELF.ADAY", "intermediates.WI.key", &["SELF.ADAY"]),
        ("XX.F", "intermediates.WI.key", &["XX.F"]),
        ("DAY", "intermediates.WI.key", &["DAY"]),
        (
            "'RK.REFDAY + NOPE'",
            "intermediates.WI.key.LO",
            &["RK.REFDAY", "NOPE"],
        ),
        (
            "{str_pad: {source: SELF.ADAY, width: 3}}",
            "intermediates.WI.key.LO",
            &["SELF.ADAY"],
        ),
    ] {
        let mut built = identifiers
            .iter()
            .map(|identifier| refused(path, identifier))
            .collect::<Vec<_>>();
        if value.starts_with("{str_pad") {
            // A padded text never compares with the int LO.
            built.insert(
                0,
                issue(
                    "incompatible_input_type",
                    "LOOKUP-9",
                    "intermediates.WI.key",
                    yamaa_adapters::json!({
                        "source": "key[LO]",
                        "expected": "str",
                        "actual": "int",
                        "intermediate": "WI",
                    }),
                ),
            );
        }
        built.push(read.clone());
        assert_issues(&rows(&keyed(value), "", "WI.NAME", "\"'x'\""), &built, &[]);
    }
    // Read by a lookup's derivations too, the row's read refuses it once the
    // rows are bound.
    let donor =
        "  - {id: D, dataset: REF, key: [STUDYID, USUBJID], derivations: {WNAME: WI.NAME}}\n";
    assert_issues(
        &rows(&(keyed("RK.REFDAY") + donor), "", "WI.NAME", "D.WNAME"),
        &[
            issue(
                "unknown_field",
                "LOOKUP-16",
                "intermediates.D.derivations.WNAME.source",
                yamaa_adapters::json!({"identifier": "RK.REFDAY", "intermediate": "D", "read": "WI"}),
            ),
            refused("intermediates.WI.key", "RK.REFDAY"),
        ],
        &[],
    );
}

/// A written aggregate `key` reports its refusals as a named intermediate's
/// do (LOOKUP-5): a key column the relation does not store, and a pair whose
/// types cannot compare, at `key`; a match value that reads no current-row
/// value at its pair, `key.<column>`, as a row context that lacks the record
/// it reads does. On the 0.5 surface they stay at the aggregate's expression.
#[test]
fn a_written_aggregate_key_reports_its_refusals_at_key() {
    let aggregate = |key: &str| format!("{{aggregate: {{expr: 'MAX(CAL.CALDY)', key: {key}}}}}");
    let at = |field: &str| format!("columns.W.derivation.aggregate.{field}");
    let unknown = |path: &str, identifier: &str| {
        issue(
            "unknown_field",
            "LOOKUP-5",
            path,
            yamaa_adapters::json!({"identifier": identifier}),
        )
    };
    let typed = |source: &str, expected: &str| {
        issue(
            "incompatible_input_type",
            "LOOKUP-9",
            &at("key"),
            yamaa_adapters::json!({"source": source, "expected": expected, "actual": "str"}),
        )
    };
    // Each key in the column of both row templates, then in a spec built
    // from the clinic records alone, where the clinic record is the row's.
    for (key, built, alone) in [
        ("{SID: ADAY}", vec![typed("ADAY", "int")], true),
        ("{SID: DAY}", vec![unknown(&at("key.SID"), "DAY")], true),
        ("{SID: RK.F}", vec![unknown(&at("key.SID"), "RK.F")], true),
        (
            "{SID: SELF.SID}",
            vec![unknown(&at("key.SID"), "SELF.SID")],
            true,
        ),
        (
            "{SID: 'DAY + 0'}",
            vec![
                typed("key[SID]", "int or float"),
                unknown(&at("key.SID"), "DAY"),
            ],
            true,
        ),
        // A home row has no clinic record, as before.
        (
            "{SID: CLINIC.SID}",
            vec![unknown(&at("key.SID"), "CLINIC.SID")],
            false,
        ),
    ] {
        assert_issues(&rows("", &aggregate(key), "", ""), &built, &[]);
        if alone {
            assert_issues(&base("", &aggregate(key)), &built, &[]);
        }
    }
    // A key column the relation does not store is decided once it is read,
    // and the pair's match value is still read.
    assert_issues(
        &base("", &aggregate("[STUDYID, DAY]")),
        &[
            unknown(&at("key"), "CAL.DAY"),
            unknown(&at("key.DAY"), "DAY"),
        ],
        &[unknown(&at("key"), "CAL.DAY")],
    );
    // A 0.5 key keeps the aggregate's expression path.
    let specification = "schema_version: \"1.0\"\n\
        domain: OUT\n\
        keys: [STUDYID, USUBJID, SEQ]\n\
        input:\n\
        \x20 CLINIC: {path: clinic.csv, types: {SEQ: int, DAY: int}}\n\
        \x20 CAL: {path: cal.csv, types: {CALDY: int}}\n\
        base: CLINIC\n\
        output: {path: out.csv, columns: [STUDYID, USUBJID, SEQ, W]}\n\
        columns:\n\
        \x20 - {name: STUDYID, type: str, derivation: CLINIC.STUDYID}\n\
        \x20 - {name: USUBJID, type: str, derivation: CLINIC.USUBJID}\n\
        \x20 - {name: SEQ, type: int, derivation: CLINIC.SEQ}\n\
        \x20 - {name: W, type: int, derivation: {aggregate: {expr: 'MAX(CAL.CALDY)', key: {SID: DAY}}}}\n";
    let fixture = Fixture::new(specification);
    let (_, built) = fixture.issues(Language::Python);
    assert_eq!(built, [unknown(&at("expr"), "DAY")], "{specification}");
}

/// A 1.0 specification built from CLINIC's records whose W is D.WNAME, read
/// from D, REF's record of the subject, which derives WNAME from WI, the
/// windows `wi` declares; RK reads OTHER's record of the subject on `rk`.
/// With `row`, the rows read WI too, in the column R.
fn donor(wi: &str, rk: &str, row: bool) -> String {
    let (output, read) = if row {
        (", R", "  - {name: R, type: str, derivation: WI.NAME}\n")
    } else {
        ("", "")
    };
    format!(
        "yamaa: \"1.0\"\n\
         type: spec\n\
         domain: OUT\n\
         keys: [STUDYID, USUBJID, SEQ]\n\
         input:\n\
         \x20 CLINIC: {{path: clinic.csv, types: {{SEQ: int, DAY: int}}}}\n\
         \x20 REF: {{path: ref.csv, types: {{REFDAY: int}}}}\n\
         \x20 OTHER: {{path: other.csv, types: {{OTHDAY: int}}}}\n\
         \x20 WIN: {{path: win.csv, types: {{LO: int, HI: int}}}}\n\
         base: CLINIC\n\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, SEQ, W{output}]}}\n\
         intermediates:\n\
         \x20 - {{id: RK, dataset: OTHER, key: {rk}}}\n\
         \x20 - {{id: WI, dataset: WIN, {wi}}}\n\
         \x20 - {{id: D, dataset: REF, key: [STUDYID, USUBJID], derivations: {{WNAME: WI.NAME}}}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str, derivation: CLINIC.STUDYID}}\n\
         \x20 - {{name: USUBJID, type: str, derivation: CLINIC.USUBJID}}\n\
         \x20 - {{name: SEQ, type: int, derivation: CLINIC.SEQ}}\n\
         \x20 - {{name: W, type: str, derivation: D.WNAME}}\n\
         {read}"
    )
}

/// WI's range whose value is `value`.
fn window(value: &str) -> String {
    format!("key: [STUDYID], between: {{value: {value}, lower: LO, upper: HI}}")
}

/// The refusal at D's derivation of a read of WI that the donor record cannot
/// give, naming the read `identifier` (LOOKUP-16).
fn donor_refusal(identifier: &str) -> Value {
    issue(
        "unknown_field",
        "LOOKUP-16",
        "intermediates.D.derivations.WNAME.source",
        yamaa_adapters::json!({"identifier": identifier, "intermediate": "D", "read": "WI"}),
    )
}

/// In a donor read a bare range value is the donor record's field, and so is
/// one qualified by the donor's dataset; a lookup-qualified one reads that
/// lookup's column for the donor record (LOOKUP-16). The subject's reference
/// day 12 is in LATE, and its OTHER day 3 in EARLY, for each clinic row.
#[test]
fn a_donor_read_takes_its_range_value_from_the_donor_record() {
    for (value, csv) in [
        (
            "REFDAY",
            "STUDYID,USUBJID,SEQ,W\nS,S-1,1,LATE\nS,S-1,2,LATE\n",
        ),
        (
            "REF.REFDAY",
            "STUDYID,USUBJID,SEQ,W\nS,S-1,1,LATE\nS,S-1,2,LATE\n",
        ),
        (
            "RK.OTHDAY",
            "STUDYID,USUBJID,SEQ,W\nS,S-1,1,EARLY\nS,S-1,2,EARLY\n",
        ),
    ] {
        let fixture = Fixture::new(&donor(&window(value), "[STUDYID, USUBJID]", false));
        for host in [Language::Python, Language::R] {
            assert_eq!(
                fixture.issues(host),
                (Vec::new(), Vec::new()),
                "{value} ({host:?})"
            );
        }
        assert_eq!(fixture.saved(), csv, "{value}");
    }
}

/// A donor read gives the donor record alone: a range value, a correlated
/// filter field or a key value of a lookup a range value reads that names
/// another input's record fails at the reading derivation (LOOKUP-16), where
/// it read whatever record stood at the donor record's position. A bare name
/// the donor record does not store fails there too, which only its records
/// decide.
#[test]
fn a_donor_read_refuses_the_record_of_another_input() {
    for (wi, rk, identifier) in [
        (window("CLINIC.DAY"), "[STUDYID, USUBJID]", "CLINIC.DAY"),
        (window("OTHER.OTHDAY"), "[STUDYID, USUBJID]", "OTHER.OTHDAY"),
        (
            "key: [STUDYID], filter: \"WIN.LO <= CLINIC.DAY\", order_by: [WIN.LO], keep: first"
                .to_owned(),
            "[STUDYID, USUBJID]",
            "CLINIC.DAY",
        ),
        (
            window("RK.OTHDAY"),
            "{STUDYID: STUDYID, USUBJID: CLINIC.USUBJID}",
            "CLINIC.USUBJID",
        ),
    ] {
        assert_issues(&donor(&wi, rk, false), &[donor_refusal(identifier)], &[]);
    }
    let unstored = donor_refusal("NOPE");
    assert_issues(
        &donor(&window("NOPE"), "[STUDYID, USUBJID]", false),
        std::slice::from_ref(&unstored),
        std::slice::from_ref(&unstored),
    );
}

/// A bare range value that no column takes stays the donor record's field
/// for a donor read, and names no value of a row that reads the lookup
/// (LOOKUP-9), once the rows are bound.
#[test]
fn a_bare_range_value_of_a_donor_read_names_no_value_of_a_row() {
    let row = issue(
        "unknown_field",
        "LOOKUP-9",
        "intermediates.WI.between.value",
        yamaa_adapters::json!({"identifier": "REFDAY", "intermediate": "WI"}),
    );
    assert_issues(
        &donor(&window("REFDAY"), "[STUDYID, USUBJID]", true),
        &[row],
        &[],
    );
}

/// The exposure records with a dose drive the `dosed` rows and the subjects
/// the `subject` rows; W, a lookup of the subject's periods, matches the one
/// holding `value`, and each template's APERIOD is `dosed` and `subject`.
fn dosed(value: &str, dosed: &str, subject: &str) -> String {
    format!(
        "yamaa: \"1.0\"\n\
         type: spec\n\
         domain: OUT\n\
         keys: [STUDYID, USUBJID, ASEQ]\n\
         input:\n\
         \x20 SUBJ: {{path: subjects.csv, types: {{EXSTDT: date}}}}\n\
         \x20 EX: {{path: ex.csv, types: {{EXSEQ: int, EXDOSE: int, EXSTDT: date}}}}\n\
         \x20 PERIODS: {{path: periods.csv, types: {{APERIOD: int, APERSDT: date, APEREDT: date}}, key: [STUDYID, USUBJID]}}\n\
         output: {{path: out.csv, columns: [STUDYID, USUBJID, ASEQ, APERIOD]}}\n\
         intermediates:\n\
         \x20 - {{id: DOSED, dataset: EX, filter: \"EX.EXDOSE > 0\"}}\n\
         \x20 - {{id: W, dataset: PERIODS, key: [STUDYID, USUBJID], between: {{value: {value}, lower: APERSDT, upper: APEREDT}}, order_by: [PERIODS.APERIOD], keep: first}}\n\
         columns:\n\
         \x20 - {{name: STUDYID, type: str}}\n\
         \x20 - {{name: USUBJID, type: str}}\n\
         \x20 - {{name: ASEQ, type: int}}\n\
         \x20 - {{name: APERIOD, type: int}}\n\
         rows:\n\
         \x20 - id: dosed\n\
         \x20   dataset: DOSED\n\
         \x20   derivations: {{STUDYID: DOSED.STUDYID, USUBJID: DOSED.USUBJID, ASEQ: DOSED.EXSEQ, APERIOD: {dosed}}}\n\
         \x20 - id: subject\n\
         \x20   dataset: SUBJ\n\
         \x20   derivations: {{STUDYID: SUBJ.STUDYID, USUBJID: SUBJ.USUBJID, ASEQ: 0, APERIOD: {subject}}}\n"
    )
}

/// A named intermediate binds once for every row that reads it, so a range
/// value qualified by the intermediate that builds a reading row cannot read
/// that row's record, and is unsupported (#3055); before, it looked the
/// intermediate up by a key it does not declare. For rows another input
/// builds, the read is a keyed lookup of a row driver, which declares no key
/// (LOOKUP-3, ROW-2), as any other read of it is.
#[test]
fn a_range_value_never_reads_the_intermediate_that_builds_its_row() {
    let unsupported = yamaa_adapters::json!({
        "phase": "planning",
        "condition": "unsupported_operation",
        "rule": null,
        "spec_paths": ["intermediates.W.between.value"],
        "context": {"operation": "intermediate_driver_range_value"},
    });
    let keyless = issue(
        "missing_required_field",
        "LOOKUP-3",
        "intermediates.DOSED.key",
        yamaa_adapters::json!({"field": "key", "class": "intermediate_class"}),
    );
    for (dosed_rows, subject_rows, expected, label) in [
        ("W.APERIOD", "null", &unsupported, "the dosed rows"),
        (
            "W.APERIOD",
            "W.APERIOD",
            &unsupported,
            "both templates' rows",
        ),
        ("null", "W.APERIOD", &keyless, "the subject rows"),
    ] {
        let specification = dosed("DOSED.EXSTDT", dosed_rows, subject_rows);
        let fixture = Fixture::new(&specification);
        for host in [Language::Python, Language::R] {
            let (checked, built) = fixture.issues(host);
            assert_eq!(
                built,
                std::slice::from_ref(expected),
                "domain, {label} ({host:?})"
            );
            assert_eq!(checked, built, "check, {label} ({host:?})");
        }
    }
}

/// A range whose value and bounds cannot compare names the source binding's
/// variable, as each other refusal of its range does; an aggregate's names no
/// owner (LOOKUP-9). On the 0.5 surface a source binding's names none.
#[test]
fn a_source_binding_s_range_types_refusal_names_its_variable() {
    let types = |path: &str, variable: Option<&str>| {
        let mut context = yamaa_core::json::Map::new();
        for (key, value) in [
            ("value_type", "str"),
            ("lower_type", "int"),
            ("upper_type", "int"),
        ] {
            context.insert(key.into(), Value::String(value.into()));
        }
        if let Some(variable) = variable {
            context.insert("variable".into(), Value::String(variable.into()));
        }
        issue(
            "incomparable_range_types",
            "LOOKUP-9",
            path,
            Value::Object(context),
        )
    };
    let source = "{source: {variable: WIN.NAME, between: {value: STUDYID, lower: LO, upper: HI}}}";
    let aggregate =
        "{aggregate: {expr: 'MAX(WIN.NAME)', between: {value: STUDYID, lower: LO, upper: HI}}}";
    for (specification, expected) in [
        (
            rows("", source, "", ""),
            types("columns.W.derivation.source.between", Some("WIN.NAME")),
        ),
        (
            rows("", "", source, "\"'x'\""),
            types("rows.clinic.derivations.W.source.between", Some("WIN.NAME")),
        ),
        (
            base("", source),
            types("columns.W.derivation.source.between", Some("WIN.NAME")),
        ),
        (
            base("", aggregate),
            types("columns.W.derivation.aggregate.between", None),
        ),
    ] {
        assert_issues(&specification, &[expected], &[]);
    }
}

/// On the 0.5 surface a donor read keeps the rules at the `yamaa-v0.5.0` tag
/// (D75): a bare range value that no column takes is a field of the driver's
/// record, which REF's REFDAY is not, and a source binding's type refusal
/// names no variable.
#[test]
fn a_0_5_donor_read_and_range_type_keep_the_0_5_rules() {
    let header = "schema_version: \"1.0\"\n\
        domain: OUT\n\
        keys: [STUDYID, USUBJID, SEQ]\n\
        input:\n\
        \x20 CLINIC: {path: clinic.csv, types: {SEQ: int, DAY: int}}\n\
        \x20 REF: {path: ref.csv, types: {REFDAY: int}}\n\
        \x20 WIN: {path: win.csv, types: {LO: int, HI: int}}\n\
        base: CLINIC\n\
        output: {path: out.csv, columns: [STUDYID, USUBJID, SEQ, W]}\n";
    let columns = "  - {name: STUDYID, type: str, derivation: CLINIC.STUDYID}\n\
        \x20 - {name: USUBJID, type: str, derivation: CLINIC.USUBJID}\n\
        \x20 - {name: SEQ, type: int, derivation: CLINIC.SEQ}\n";
    for (specification, expected) in [
        (
            format!(
                "{header}intermediates:\n\
                 \x20 - {{id: WI, dataset: WIN, key: [STUDYID], between: {{value: REFDAY, lower: LO, upper: HI}}}}\n\
                 \x20 - {{id: D, dataset: REF, key: [STUDYID, USUBJID], derivations: {{WNAME: WI.NAME}}}}\n\
                 columns:\n{columns}\
                 \x20 - {{name: W, type: str, derivation: D.WNAME}}\n"
            ),
            issue(
                "unknown_field",
                "LOOKUP-9",
                "intermediates[0].between.value",
                yamaa_adapters::json!({"identifier": "REFDAY", "intermediate": "WI"}),
            ),
        ),
        (
            format!(
                "{header}columns:\n{columns}\
                 \x20 - {{name: W, type: str, derivation: {{source: {{variable: WIN.NAME, between: {{value: STUDYID, lower: LO, upper: HI}}}}}}}}\n"
            ),
            issue(
                "incomparable_range_types",
                "LOOKUP-9",
                "columns.W.derivation.source.between",
                yamaa_adapters::json!({"value_type": "str", "lower_type": "int", "upper_type": "int"}),
            ),
        ),
    ] {
        let fixture = Fixture::new(&specification);
        let (_, built) = fixture.issues(Language::Python);
        assert_eq!(built, [expected], "{specification}");
    }
}
