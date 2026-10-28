//! Issue locations (P-ISSUE-1, D28): each case of
//! `docs/design/interface-1.0/api.md`, "Issue locations", through the public
//! engine. Every expected line and column was counted by hand in the files
//! below; each test names the text that precedes the column.
//!
//! 1. An issue without a spec path has no location, except `invalid_yaml` and
//!    `non_ascii_source`, which give the file read and the reader's position.
//! 2. A path is walked to the deepest node a layer wrote; a path into an
//!    expanded long form stops at the written node.
//! 3. Among the layers that wrote it, the one whose value took effect.
//! 4. A path that starts in the project file locates there, or in the listed
//!    file it continues into.
//! 5. `file` is absolute with forward slashes; `line` and `column` are
//!    one-based, at a mapping member's key or a list item's first character.
//! 6. A position never points into a data file.
#![cfg(feature = "yaml")]

use std::path::PathBuf;

use yamaa::{
    CancellationToken, ContextValue, Diagnostic, Engine, EngineOptions, HostLanguage,
    SubmissionOptions,
};

fn engine() -> Engine {
    Engine::new(EngineOptions::new(
        HostLanguage::Python,
        "issue-locations".into(),
        "0".into(),
    ))
}

/// A study directory outside the current directory, so a location names its
/// file by its absolute path.
struct Study(crate::support::Scratch);
impl Study {
    fn new() -> Self {
        let root = crate::support::Scratch::new("issue-locations");
        Self(root)
    }
    /// Write `lines`, each ended by LF.
    fn write(&self, name: &str, lines: &[&str]) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
        std::fs::write(path, text).unwrap();
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().to_owned()
    }
    /// A file as a location names it: absolute, with forward slashes.
    fn file(&self, name: &str) -> String {
        self.path(name).replace('\\', "/")
    }
}

/// The first issue's location as (file, line, column).
fn location(issues: &[Diagnostic]) -> Option<(String, usize, usize)> {
    let location = issues.first().expect("an issue").location.clone()?;
    let position = location.position.expect("a located node");
    Some((location.file, position.line, position.column))
}

/// The header of a specification.
const HEADER: [&str; 2] = ["yamaa: \"1.0\"", "type: spec"];

/// `lines` after [`HEADER`], and the number of header lines.
fn spec(lines: &[&str]) -> (Vec<String>, usize) {
    let mut text = HEADER.map(str::to_owned).to_vec();
    text.extend(lines.iter().map(|line| (*line).to_owned()));
    (text, HEADER.len())
}

fn write(study: &Study, name: &str, lines: &[String]) {
    study.write(name, &lines.iter().map(String::as_str).collect::<Vec<_>>());
}

/// Rule 5: ASTDY reads TRTSDT, declared after it (COLUMN-5). The path ends
/// at the expression field, `columns.ASTDY.derivation`.
#[test]
fn a_path_locates_at_the_key_of_its_deepest_written_member() {
    let study = Study::new();
    let (text, offset) = spec(&[
        "domain: ADAE",
        "keys: [USUBJID]",
        "input: {AE: {path: ae.csv, types: {TRTSDT: int}}}",
        "columns:",
        "  - {name: USUBJID, type: str, label: Subject, derivation: AE.USUBJID}",
        "  - name: ASTDY",
        "    type: int",
        "    label: Study Day",
        "    derivation: TRTSDT + 1",
        "  - {name: TRTSDT, type: int, label: Start, derivation: AE.TRTSDT}",
        "output: {path: adae.csv, columns: [USUBJID, ASTDY, TRTSDT]}",
    ]);
    write(&study, "adae.yaml", &text);
    study.write("ae.csv", &["USUBJID,TRTSDT", "S1,3"]);
    let check = engine().spec(&study.path("adae.yaml")).check().unwrap();
    let issues = check.diagnostics();
    // Line 9 of the body: the `derivation` key at column 5, after four
    // spaces.
    let column = 5;
    assert_eq!(
        location(&issues),
        Some((study.file("adae.yaml"), offset + 9, column)),
        "{:?}",
        issues
    );
    // The issue line starts with the location, and the table and the
    // JSON object carry it.
    assert!(issues[0].text().starts_with(&format!(
        "{}:{}:{column}: column ASTDY reads TRTSDT",
        study.file("adae.yaml"),
        offset + 9
    )));
    let ContextValue::Map(object) = issues[0].to_value() else {
        panic!("an issue is an object");
    };
    assert_eq!(object["file"], ContextValue::Text(study.file("adae.yaml")));
    assert_eq!(object["line"], ContextValue::Int((offset + 9) as i64));
    assert_eq!(object["column"], ContextValue::Int(column as i64));
    assert_eq!(check.issues().rows(), 1);
}

/// Rule 2: a path into an expanded long form stops at the written node. AGE
/// reads `RAW.AGEYRS`, which the input does not hold, through the shorthand
/// that expands to `source`; the issue cites `columns.AGE.derivation.source`
/// and is located at the `derivation` key that wrote the shorthand. A list
/// item a path names by identity is located at its first character.
#[test]
fn a_path_into_an_expanded_long_form_stops_at_the_written_node() {
    let body = [
        "domain: DM",
        "keys: [USUBJID]",
        "input: {RAW: raw.csv}",
        "columns:",
        "  - name: USUBJID",
        "    type: str",
        "    label: Subject",
        "    derivation: RAW.USUBJID",
        "  - name: AGE",
        "    type: int",
        "    label: Age",
        "    derivation: RAW.AGEYRS",
        "output: {path: dm.csv, columns: [USUBJID, AGE]}",
    ];
    let study = Study::new();
    let (text, offset) = spec(&body);
    write(&study, "dm.yaml", &text);
    study.write("raw.csv", &["USUBJID,AGE", "S1,30"]);
    let run = engine().spec(&study.path("dm.yaml")).run().unwrap();
    let issues = run.diagnostics();
    assert_eq!(
        issues[0].spec_path,
        ["columns.AGE.derivation.source"],
        "{issues:?}"
    );
    // Line 12 of the body, after four spaces.
    assert_eq!(
        location(&issues),
        Some((study.file("dm.yaml"), offset + 12, 5))
    );
}

/// Rule 3: a value a child layer writes is located in the child, one it
/// inherits in the parent. RATIO divides by zero at run time, in the
/// derivation the parent writes, unless the child writes its own; each
/// writes the expression as the field's text, located at its `derivation`
/// key, after four spaces.
#[test]
fn a_layered_issue_is_located_in_the_layer_whose_value_took_effect() {
    let (path, column) = ("columns.RATIO.derivation", 5);
    let parent = [
        "domain: ADSL",
        "keys: [USUBJID]",
        "input: {DM: {path: dm.csv, types: {AGE: int, ZERO: int}}}",
        "columns:",
        "  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}",
        "  - name: RATIO",
        "    type: float",
        "    label: Ratio",
        "    derivation: AGE / ZERO",
        "  - {name: AGE, type: int, label: Age, derivation: DM.AGE}",
        "  - {name: ZERO, type: int, label: Zero, derivation: DM.ZERO}",
        "output: {path: adsl.csv, columns: [USUBJID, RATIO, AGE, ZERO]}",
    ];
    let relabelled = [
        "parent: parent.yaml",
        "columns:",
        "  - name: RATIO",
        "    label: Age Ratio",
    ];
    let rederived = [
        "parent: parent.yaml",
        "columns:",
        "  - name: RATIO",
        "    derivation: AGE / (ZERO * 2)",
    ];
    let study = Study::new();
    let (text, offset) = spec(&parent);
    write(&study, "parent.yaml", &text);
    let (text, child_offset) = spec(&relabelled);
    write(&study, "relabelled.yaml", &text);
    let (text, _) = spec(&rederived);
    write(&study, "rederived.yaml", &text);
    study.write("dm.csv", &["USUBJID,AGE,ZERO", "S1,30,0"]);
    let first = |name: &str| {
        let run = engine().spec(&study.path(name)).run().unwrap();
        let issues = run.diagnostics();
        assert_eq!(issues[0].spec_path, [path], "{issues:?}");
        location(&issues)
    };
    // Line 9 of the parent's body.
    assert_eq!(
        first("relabelled.yaml"),
        Some((study.file("parent.yaml"), offset + 9, column))
    );
    // Line 4 of the child's own body.
    assert_eq!(
        first("rederived.yaml"),
        Some((study.file("rederived.yaml"), child_offset + 4, column))
    );
}

/// A shorthand contributes the same source variable as its long form. A
/// label-only child keeps the parent's source; a text expression replaces
/// it whole, so the parent's filter no longer applies.
#[test]
fn a_child_source_shorthand_is_located_in_the_child_over_a_structured_parent() {
    let study = Study::new();
    let (text, parent_offset) = spec(&[
        "domain: IT",
        "keys: [ID]",
        "input: {RAW: raw.csv}",
        "output: {path: out.csv, columns: [ID, V]}",
        "columns:",
        "  - {name: ID, type: str, label: Identifier, derivation: RAW.ID}",
        "  - name: V",
        "    type: str",
        "    label: Value",
        "    derivation:",
        "      source:",
        "        variable: RAW.OLD",
        "        filter: RAW.ID IS NOT NULL",
    ]);
    write(&study, "parent.yaml", &text);
    study.write("raw.csv", &["ID,OLD", "S1,old"]);
    for derivation in [
        "    derivation: RAW.NEW",
        "    derivation: {source: {variable: RAW.NEW, filter: RAW.ID IS NOT NULL}}",
    ] {
        let (text, offset) = spec(&["parent: parent.yaml", "columns:", "  - name: V", derivation]);
        write(&study, "child.yaml", &text);
        let run = engine().spec(&study.path("child.yaml")).run().unwrap();
        let issues = run.diagnostics();
        assert_eq!(issues[0].code.as_str(), "unknown_field", "{issues:?}");
        assert_eq!(issues[0].spec_path, ["columns.V.derivation.source"]);
        // Shorthand is at its derivation key; the long form at source.
        let column = if derivation == "    derivation: RAW.NEW" {
            5
        } else {
            18
        };
        assert_eq!(
            location(&issues),
            Some((study.file("child.yaml"), offset + 4, column))
        );
    }
    // A label-only contribution keeps the parent's source location.
    let (text, _) = spec(&[
        "parent: parent.yaml",
        "columns:",
        "  - name: V",
        "    label: Renamed",
    ]);
    write(&study, "child.yaml", &text);
    study.write("raw.csv", &["ID", "S1"]);
    let run = engine().spec(&study.path("child.yaml")).run().unwrap();
    assert_eq!(
        location(&run.diagnostics()),
        Some((study.file("parent.yaml"), parent_offset + 11, 7))
    );

    // Exercise the source's inherited sibling, rather than assigning
    // every path below derivation to the latest layer indiscriminately.
    let parent = std::fs::read_to_string(study.path("parent.yaml"))
        .unwrap()
        .replace(
            "filter: RAW.ID IS NOT NULL",
            "filter: RAW.MISSING IS NOT NULL",
        );
    std::fs::write(study.path("parent.yaml"), parent).unwrap();
    let (text, _) = spec(&[
        "parent: parent.yaml",
        "columns:",
        "  - name: V",
        "    derivation: RAW.NEW",
    ]);
    write(&study, "child.yaml", &text);
    study.write("raw.csv", &["ID,NEW", "S1,new"]);
    let run = engine().spec(&study.path("child.yaml")).run().unwrap();
    let issues = run.diagnostics();
    assert!(issues.is_empty(), "{issues:?}");
}

/// A child's output composes field by field with its parent's (LAYER-5), so
/// it keeps the inherited report; a clear removes the named inherited field,
/// which then locates at that clear, never at the parent's old key, even
/// where a later layer writes the output again (LAYER-9).
#[test]
fn cleared_fields_do_not_locate_in_overridden_parent_values() {
    let study = Study::new();
    let (text, _) = spec(&[
        "domain: IT",
        "keys: [ID]",
        "input: {RAW: raw.csv}",
        "output:",
        "  path: out.csv",
        "  columns: [ID]",
        "  report: report.csv",
        "columns:",
        "  - {name: ID, type: str, label: Identifier, derivation: RAW.ID}",
        "verifications:",
        "  - row_count: {min: 1, severity: warning}",
    ]);
    write(&study, "parent.yaml", &text);
    study.write("raw.csv", &["ID", "S1"]);
    let check = |name: &str| {
        engine()
            .spec(&study.path(name))
            .check()
            .unwrap()
            .diagnostics()
    };
    assert!(check("parent.yaml").is_empty());
    let (text, _) = spec(&[
        "parent: parent.yaml",
        "output:",
        "  path: child.csv",
        "  columns: [ID]",
    ]);
    write(&study, "child.yaml", &text);
    let issues = check("child.yaml");
    assert!(
        issues.is_empty(),
        "a child output keeps the inherited report: {issues:?}"
    );
    for (cleared, code, path) in [
        ("output.report", "missing_report", "output.report"),
        (
            "columns.ID.derivation",
            "missing_derivation",
            "columns.ID.derivation",
        ),
    ] {
        let (text, offset) = spec(&["parent: parent.yaml", &format!("clear: [{cleared}]")]);
        write(&study, "cleared.yaml", &text);
        let issues = check("cleared.yaml");
        assert_eq!(issues[0].code.as_str(), code, "{issues:?}");
        assert_eq!(issues[0].spec_path, [path]);
        // The clear entry follows `clear: [` (eight characters).
        assert_eq!(
            location(&issues),
            Some((study.file("cleared.yaml"), offset + 2, 9))
        );
    }
    // The grandchild's output merges with the one `cleared.yaml` left
    // without a report, so the report stays removed, by that clear.
    let (text, offset) = spec(&["parent: parent.yaml", "clear: [output.report]"]);
    write(&study, "cleared.yaml", &text);
    let (text, _) = spec(&[
        "parent: cleared.yaml",
        "output:",
        "  path: grandchild.csv",
        "  columns: [ID]",
    ]);
    write(&study, "grandchild.yaml", &text);
    let issues = check("grandchild.yaml");
    let first = issues
        .first()
        .map(|issue| (issue.code.as_str(), issue.spec_path.clone()));
    assert_eq!(
        (first, location(&issues)),
        (
            Some(("missing_report", vec!["output.report".to_owned()])),
            Some((study.file("cleared.yaml"), offset + 2, 9))
        ),
        "a report cleared below a later output: {issues:?}"
    );
}

/// Rule 4: a path that starts in the project file locates there, or in the
/// listed file it continues into. Two listings name one dataset; the first
/// path, `sdtm.specs[0]`, is the first listing, at its first character. A
/// 1.0 submission of an SDTM section lists a specification of model ADaM, at
/// `sdtm.specs[0].model`, which continues into that specification.
#[test]
fn a_project_path_locates_in_the_project_or_the_file_it_lists() {
    let study = Study::new();
    let dm = [
        "yamaa: \"1.0\"",
        "type: spec",
        "domain: DM",
        "keys: [USUBJID]",
        "input: {RAW: raw.csv}",
        "columns:",
        "  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}",
        "output: {path: dm.csv, columns: [USUBJID]}",
    ];
    study.write("dm.yaml", &dm);
    study.write("again.yaml", &dm);
    study.write(
        "project.yaml",
        &[
            "yamaa: \"1.0\"",
            "type: project",
            "sdtm:",
            "  specs:",
            "    - dm.yaml",
            "    - again.yaml",
        ],
    );
    let check = engine()
        .project(&study.path("project.yaml"))
        .check()
        .unwrap();
    let issues = check.diagnostics();
    assert_eq!(issues[0].spec_path[0], "sdtm.specs[0]", "{issues:?}");
    // Line 5, after `    - `.
    assert_eq!(location(&issues), Some((study.file("project.yaml"), 5, 7)));

    let study = Study::new();
    study.write(
        "project.yaml",
        &[
            "yamaa: \"1.0\"",
            "type: project",
            "study:",
            "  id: CATH",
            "  name: CATH",
            "  description: A study",
            "  protocol_name: CATH-01",
            "sdtm:",
            "  standards:",
            "    - {name: SDTMIG, version: \"3.4\", status: Final}",
            "  specs:",
            "    - spec.yaml",
            "  define: define.xml",
            "  context: Other",
        ],
    );
    study.write(
        "spec.yaml",
        &[
            "yamaa: \"1.0\"",
            "type: spec",
            "model: ADaM",
            "domain: ADSL",
            "keys: [USUBJID]",
            "input: {RAW: raw.csv}",
            "columns:",
            "  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}",
            "output: {path: adsl.csv, columns: [USUBJID]}",
        ],
    );
    let submission = engine()
        .project(&study.path("project.yaml"))
        .section("sdtm")
        .submission(&SubmissionOptions::new("0.4.0").with_created("2026-09-11T00:00:00"))
        .unwrap();
    let issues = submission.diagnostics();
    assert_eq!(issues[0].spec_path, ["sdtm.specs[0].model"], "{issues:?}");
    // The specification's `model` key, line 3.
    assert_eq!(location(&issues), Some((study.file("spec.yaml"), 3, 1)));
}

/// Rule 5: a finding in the header of the producer a `schema` link names
/// names that producer as `source`, so it is located in the producer even
/// where the consumer writes the same field: a malformed version, where the
/// consumer's own `yamaa` ties with the producer's, on the check and run
/// routes of each host (SCHEMA-2, #3581).
#[test]
fn a_linked_producer_s_header_finding_locates_in_the_producer() {
    let token = CancellationToken::new();
    let study = Study::new();
    let (consumer, _) = spec(&[
        "domain: DM",
        "keys: [USUBJID]",
        "input: {SRC: {path: produced.csv, schema: producer.yaml}}",
        "columns:",
        "  - {name: USUBJID, type: str, label: Subject, derivation: SRC.USUBJID}",
        "output: {path: dm.csv, columns: [USUBJID]}",
    ]);
    write(&study, "dm.yaml", &consumer);
    study.write("producer.yaml", &["yamaa: 1.0", "type: spec"]);
    for host in [HostLanguage::Python, HostLanguage::R] {
        let engine = Engine::new(EngineOptions::new(
            host,
            "issue-locations".into(),
            "0".into(),
        ));
        let entry = study.path("dm.yaml");
        let checked = engine.spec(&entry).cancel_with(&token).check().unwrap();
        let run = engine.spec(&entry).cancel_with(&token).run().unwrap();
        for issues in [checked.diagnostics(), run.diagnostics()] {
            let first = issues.first().expect("an issue");
            assert_eq!(
                (
                    first.code.as_str(),
                    first.spec_path.as_slice(),
                    first.context.get("source"),
                    location(&issues),
                ),
                (
                    "invalid_field_type",
                    ["yamaa".to_string()].as_slice(),
                    Some(&ContextValue::Text(study.file("producer.yaml"))),
                    // The producer's `yamaa` key, line 1.
                    Some((study.file("producer.yaml"), 1, 1)),
                ),
                "{host:?}: {issues:?}"
            );
        }
    }
}

/// Rule 6: a position never points into a data file. AGE is `NA` in the
/// second record of the input: the issue is located at the declaration of
/// AGE's type in the specification, and the record stays in the context.
#[test]
fn a_location_never_points_into_a_data_file() {
    let body = [
        "domain: DM",
        "keys: [USUBJID]",
        "input: {RAW: {path: raw.csv, types: {AGE: int}}}",
        "columns:",
        "  - {name: USUBJID, type: str, label: Subject, derivation: RAW.USUBJID}",
        "  - {name: AGE, type: int, label: Age, derivation: RAW.AGE}",
        "output: {path: dm.csv, columns: [USUBJID, AGE]}",
    ];
    let study = Study::new();
    let (text, offset) = spec(&body);
    write(&study, "dm.yaml", &text);
    study.write("raw.csv", &["USUBJID,AGE", "S1,30", "S2,NA"]);
    let run = engine().spec(&study.path("dm.yaml")).run().unwrap();
    let issues = run.diagnostics();
    assert_eq!(issues[0].spec_path, ["input.RAW.types.AGE"], "{issues:?}");
    // Line 3 of the body, after `input: {RAW: {path: raw.csv, types: {`
    // (37 characters).
    assert_eq!(
        location(&issues),
        Some((study.file("dm.yaml"), offset + 3, 38))
    );
    assert_eq!(
        issues[0].context.get("value"),
        Some(&ContextValue::Text("NA".into()))
    );
}

/// Rule 1: an issue without a spec path has no location. `invalid_yaml` and
/// `non_ascii_source` have none either, yet are located in the file the
/// reader refused, at the position it reported, which their context names;
/// in a parent layer, that parent.
#[test]
fn only_a_refused_file_locates_an_issue_without_a_written_node() {
    let study = Study::new();
    study.write(
        "project.yaml",
        &["yamaa: \"1.0\"", "type: project", "sdtm:", "  specs: []"],
    );
    let failure = engine()
        .project(&study.path("project.yaml"))
        .spec("ADXX")
        .unwrap_err();
    assert!(failure.diagnostics[0].spec_path.is_empty());
    assert_eq!(failure.diagnostics[0].location, None);

    // The flow sequence is never closed: the reader stops at line 4, column 1.
    study.write(
        "invalid.yaml",
        &["yamaa: \"1.0\"", "type: spec", "domain: [ADSL"],
    );
    let check = engine().spec(&study.path("invalid.yaml")).check().unwrap();
    let issues = check.diagnostics();
    assert_eq!(location(&issues), Some((study.file("invalid.yaml"), 4, 1)));
    assert_eq!(issues[0].context.get("line"), Some(&ContextValue::Int(4)));
    assert_eq!(issues[0].context.get("column"), Some(&ContextValue::Int(1)));

    // The comment's sixth character, after `# caf`, is not ASCII.
    std::fs::write(
        study.0.join("accented.yaml"),
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADSL\n# caf\u{e9}\n",
    )
    .unwrap();
    let check = engine().spec(&study.path("accented.yaml")).check().unwrap();
    assert_eq!(
        location(&check.diagnostics()),
        Some((study.file("accented.yaml"), 4, 6))
    );

    // A specification whose project file the reader refuses, read with it
    // and read alone: the project file is the file refused.
    study.write(
        "project.yaml",
        &[
            "yamaa: \"1.0\"",
            "type: project",
            "sdtm: {specs: [accented.yaml",
        ],
    );
    let check = engine()
        .spec(&study.path("accented.yaml"))
        .project(&study.path("project.yaml"))
        .check()
        .unwrap();
    assert_eq!(
        location(&check.diagnostics()),
        Some((study.file("project.yaml"), 4, 1))
    );
    let check = engine()
        .project(&study.path("project.yaml"))
        .check()
        .unwrap();
    assert_eq!(
        location(&check.diagnostics()),
        Some((study.file("project.yaml"), 4, 1))
    );
}

/// A parent layer the reader refuses gives the issue an entry it refuses
/// gives, `invalid_yaml` or `non_ascii_source` at `$` with the reader's
/// position, located in that parent, on every route that reads the
/// specification (SCHEMA-3, SPEC-8; #3519).
#[test]
fn a_parent_the_reader_refuses_locates_its_issue_in_that_parent() {
    let token = CancellationToken::new();
    let study = Study::new();
    let (child, _) = spec(&["parent: refused.yaml"]);
    write(&study, "child.yaml", &child);
    let entry = study.path("child.yaml");
    // The flow sequence is never closed: the reader stops at the line
    // after it, at column 1.
    let (invalid, lines) = spec(&["domain: [ADSL"]);
    write(&study, "refused.yaml", &invalid);
    // The comment's sixth character, after `# caf`, is not ASCII.
    let (accented, _) = spec(&["domain: ADSL", "# caf\u{e9}"]);
    for (text, condition, rule, line, column) in [
        (invalid, "invalid_yaml", "SCHEMA-3", lines + 2, 1),
        (accented, "non_ascii_source", "SPEC-8", lines + 2, 6),
    ] {
        write(&study, "refused.yaml", &text);
        let checked = engine().spec(&entry).cancel_with(&token).check().unwrap();
        let ran = engine().spec(&entry).cancel_with(&token).run().unwrap();
        let resolved = engine().spec(&entry).resolve().unwrap_err();
        for issues in [
            checked.diagnostics(),
            ran.diagnostics(),
            resolved.diagnostics,
        ] {
            let [issue] = issues.as_slice() else {
                panic!("one issue: {issues:?}")
            };
            assert_eq!(
                (issue.code.as_str(), issue.rule.as_deref()),
                (condition, Some(rule)),
                "{condition}"
            );
            assert_eq!(issue.spec_path, ["$"]);
            assert_eq!(
                issue.context.get("line"),
                Some(&ContextValue::Int(line as i64))
            );
            assert_eq!(
                issue.context.get("column"),
                Some(&ContextValue::Int(column as i64))
            );
            assert_eq!(
                location(&issues),
                Some((study.file("refused.yaml"), line, column)),
                "{condition}"
            );
        }
    }
}

/// Every negative benchmark whose failure belongs to its submission locates
/// its first issue, as the memory route checks for every other negative. A
/// dataset-level submission issue names the composed dataset metadata rather
/// than a positioned project node. A submission composed here also reads no
/// saved artifact, so transport failures stop earlier at the missing artifact,
/// which likewise has no authored location.
#[test]
fn every_submission_negative_locates_its_first_issue() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
    let mut cases = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().into_string().ok()?;
            let benchmark = std::fs::read_to_string(entry.path().join("benchmark.yaml")).ok()?;
            (name.starts_with("negative-") && benchmark.contains("\ndefine:")).then_some((
                name,
                entry.path(),
                benchmark,
            ))
        })
        .collect::<Vec<_>>();
    cases.sort();
    assert!(cases.len() >= 9, "{} submission negatives", cases.len());
    let field = |benchmark: &str, key: &str| {
        let line = benchmark
            .lines()
            .skip_while(|line| *line != "define:")
            .find_map(|line| line.trim_start().strip_prefix(&format!("{key}: ")))
            .unwrap_or_else(|| panic!("define names {key}"))
            .trim_matches('\'');
        line.to_owned()
    };
    let mut unlocated = Vec::new();
    for (name, case, benchmark) in cases {
        let project = case.join(field(&benchmark, "environment"));
        let submission = engine()
            .project(project.to_str().unwrap())
            .section(&field(&benchmark, "standard"))
            .submission(&SubmissionOptions::new("0.4.0").with_created(field(&benchmark, "created")))
            .unwrap();
        let issues = submission.diagnostics();
        let first = issues
            .first()
            .unwrap_or_else(|| panic!("{name} has an issue"));
        let located = first.location.as_ref().and_then(|at| at.position).is_some();
        if first.spec_path[0].starts_with("datasets.") {
            assert!(!located, "{name}: {first:?}");
            unlocated.push(name);
        } else {
            assert!(located, "{name}: {first:?}");
        }
    }
    assert_eq!(
        unlocated,
        [
            "negative-define-2-0-non-standard",
            "negative-transport-date-before-1582",
            "negative-transport-empty-dataset",
            "negative-transport-empty-string",
            "negative-transport-non-ascii",
            "negative-transport-trailing-blank"
        ]
    );
}
