//! `partial_restatement` through the public [`yamaa::lint`] facade: a layer
//! that restates a mapping it inherits, an input's `types`, an
//! intermediate's `key` or `derivations`, a row template's `derivations`, a
//! named window or a block, `output` or `submission`, and leaves out entries
//! that come back from its parent (LAYER-5, LAYER-6, LAYER-8, LAYER-9). Each
//! expected row is written by hand: its
//! position is the restated mapping's key in the child as written, its
//! entries are those the parent supplies and the child neither writes nor
//! clears, and its message is the style guide's.
#![cfg(feature = "yaml")]

use crate::support::scratch::Scratch;

use yamaa::lint;

/// The parent every case inherits: an input with field types, an output
/// with a report, a named window, a named intermediate whose `key` is a
/// mapping, a row template, a `copy` entry, which composes only once its
/// producer is read (SPEC-9) and so never keeps the other collections from
/// composing here, and submission metadata with a comment.
const PARENT: &str = r#"yamaa: "1.0"
type: spec
domain: ADTOY
keys: [USUBJID, VISIT]
input:
  SC:
    path: input/scores.csv
    types: {VISIT: int, SCORE: float}
  BL: input/baseline.csv
  ADSL: {path: adsl.csv, schema: adsl.yaml}

output:
  path: out.csv
  columns: [USUBJID, VISIT, AVAL, NOTE]
  report: report.csv

windows:
  BYSUBJ:
    group_by: [SC.USUBJID]
    order_by: [SC.VISIT]
    filter: SC.SCORE IS NOT NULL

intermediates:
  - id: BASE
    dataset: BL
    key: {USUBJID: USUBJID, VISIT: VISIT}
    derivations:
      BASESCORE: BL.SCORE
      BASENOTE: BL.COMMENT

columns:
  - copy: {dataset: ADSL, columns: [AGE]}
  - {name: USUBJID, type: str, label: Subject}
  - {name: VISIT, type: int, label: Visit}
  - {name: AVAL, type: float, label: Value}
  - {name: NOTE, type: str, label: Note, derivation: "'none'"}

rows:
  - id: scores
    dataset: SC
    derivations:
      USUBJID: SC.USUBJID
      VISIT: SC.VISIT
      AVAL: SC.SCORE
      NOTE: SC.COMMENT

submission:
  label: Scores by Visit
  class: BASIC DATA STRUCTURE
  structure: One record per subject per visit
  repeating: true
  comment: Scores the site entered
"#;

/// A child of `parent.yaml` whose sections are `body`.
fn child(body: &str) -> String {
    format!("yamaa: \"1.0\"\ntype: spec\nparent: parent.yaml\n{body}")
}

/// The row template restated without `NOTE`, as #3584 reports it.
fn restated_rows() -> String {
    child(
        "\nrows:\n  - id: scores\n    derivations:\n      USUBJID: SC.USUBJID\n      \
         VISIT: SC.VISIT\n      AVAL: SC.SCORE\n",
    )
}

/// One row of the lint table: (line, column, finding, spec path, message).
type Row = (usize, usize, String, String, String);

fn row(line: usize, column: usize, spec_path: &str, message: &str) -> Row {
    (
        line,
        column,
        "partial_restatement".to_owned(),
        spec_path.to_owned(),
        message.to_owned(),
    )
}

/// The rows `lint` gives the file `name` of `directory`, each of them in that
/// file.
fn rows(directory: &Scratch, name: &str, fix: bool) -> Vec<Row> {
    let file = directory.join(name).to_string_lossy().into_owned();
    let linted = lint(std::slice::from_ref(&file), fix).unwrap();
    let absolute = file.replace('\\', "/");
    linted
        .findings()
        .iter()
        .map(|item| {
            assert_eq!(item.file, absolute, "a finding names the linted file");
            (
                item.line,
                item.column,
                item.finding.to_owned(),
                item.spec_path.clone(),
                item.message.clone(),
            )
        })
        .collect()
}

#[test]
fn a_restated_mapping_that_leaves_inherited_entries_out_is_reported() {
    let replace = |path: &str| {
        format!("list `{path}` in `clear` to replace the mapping, or write only what this layer changes")
    };
    let cases: Vec<(&str, String, Vec<Row>)> = vec![
        (
            "a row template restated without NOTE",
            restated_rows(),
            vec![row(
                7,
                5,
                "rows.scores.derivations",
                &format!(
                    "`NOTE` comes back from the parent; {}",
                    replace("rows.scores.derivations")
                ),
            )],
        ),
        (
            "an empty mapping restates nothing it keeps",
            child("\nrows:\n  - id: scores\n    derivations: {}\n"),
            vec![row(
                7,
                5,
                "rows.scores.derivations",
                &format!(
                    "`USUBJID`, `VISIT`, `AVAL` and `NOTE` come back from the parent; {}",
                    replace("rows.scores.derivations")
                ),
            )],
        ),
        (
            "an input's types",
            child("input:\n  SC:\n    types: {VISIT: int}\n"),
            vec![row(
                6,
                5,
                "input.SC.types",
                &format!(
                    "`SCORE` comes back from the parent; {}",
                    replace("input.SC.types")
                ),
            )],
        ),
        (
            "an intermediate's key mapping",
            child("\nintermediates:\n  - id: BASE\n    key: {USUBJID: USUBJID}\n"),
            vec![row(
                7,
                5,
                "intermediates.BASE.key",
                &format!(
                    "`VISIT` comes back from the parent; {}",
                    replace("intermediates.BASE.key")
                ),
            )],
        ),
        (
            "an intermediate's derivations",
            child("\nintermediates:\n  - id: BASE\n    derivations:\n      BASESCORE: BL.SCORE\n"),
            vec![row(
                7,
                5,
                "intermediates.BASE.derivations",
                &format!(
                    "`BASENOTE` comes back from the parent; {}",
                    replace("intermediates.BASE.derivations")
                ),
            )],
        ),
        (
            "a named window, whose settings are each cleared by their own path",
            child("\nwindows:\n  BYSUBJ:\n    group_by: [SC.USUBJID]\n    order_by: [SC.SCORE]\n"),
            vec![row(
                6,
                3,
                "windows.BYSUBJ",
                "`filter` comes back from the parent; list `windows.BYSUBJ.filter` in `clear` \
                 to drop it, or write only what this layer changes",
            )],
        ),
        (
            "a named window written empty",
            child("\nwindows:\n  BYSUBJ: {}\n"),
            vec![row(
                6,
                3,
                "windows.BYSUBJ",
                "`group_by`, `order_by` and `filter` come back from the parent; list \
                 `windows.BYSUBJ.group_by`, `windows.BYSUBJ.order_by` and \
                 `windows.BYSUBJ.filter` in `clear` to drop them, or write only what this \
                 layer changes",
            )],
        ),
        (
            "an output restated with its path, whose fields are each cleared by their own path",
            child("\noutput:\n  path: out.csv\n  columns: [USUBJID, AVAL]\n"),
            vec![row(
                5,
                1,
                "output",
                "`report` comes back from the parent; list `output.report` in `clear` to \
                 drop it, or write only what this layer changes",
            )],
        ),
        (
            "submission metadata restated without its comment",
            child("\nsubmission:\n  label: Scores by Visit\n  repeating: true\n"),
            vec![row(
                5,
                1,
                "submission",
                "`comment` comes back from the parent; list `submission.comment` in `clear` \
                 to drop it, or write only what this layer changes",
            )],
        ),
        (
            "a required field that comes back is written, not cleared, so it is not named",
            child("\noutput:\n  columns: [USUBJID, VISIT, AVAL, NOTE]\n  report: own.csv\n"),
            vec![],
        ),
        (
            "changing one output field is a patch",
            child("\noutput:\n  columns: [USUBJID, AVAL]\n"),
            vec![],
        ),
        (
            "clearing the output field left out removes it",
            child("clear:\n  - output.report\n\noutput:\n  path: out.csv\n  columns: [USUBJID, AVAL]\n"),
            vec![],
        ),
        (
            "changing one derivation is a patch",
            child("\nrows:\n  - id: scores\n    derivations:\n      AVAL: SC.SCORE * 2\n"),
            vec![],
        ),
        (
            "adding a derivation is a patch",
            child("\nrows:\n  - id: scores\n    derivations:\n      BASE: BASE.BASESCORE\n"),
            vec![],
        ),
        (
            "changing one window setting is a patch",
            child("\nwindows:\n  BYSUBJ:\n    order_by: [SC.SCORE]\n"),
            vec![],
        ),
        (
            "restating every entry leaves none out",
            child(
                "\nrows:\n  - id: scores\n    derivations:\n      USUBJID: SC.USUBJID\n      \
                 VISIT: SC.VISIT\n      AVAL: SC.SCORE\n      NOTE: SC.COMMENT\n",
            ),
            vec![],
        ),
        (
            "clearing the mapping replaces it",
            restated_rows().replace(
                "parent: parent.yaml\n",
                "parent: parent.yaml\nclear:\n  - rows.scores.derivations\n",
            ),
            vec![],
        ),
        (
            "clearing the entry left out removes it",
            restated_rows().replace(
                "parent: parent.yaml\n",
                "parent: parent.yaml\nclear:\n  - rows.scores.derivations.NOTE\n",
            ),
            vec![],
        ),
        (
            "a key written as a list replaces the mapping",
            child("\nintermediates:\n  - id: BASE\n    key: [USUBJID]\n"),
            vec![],
        ),
        (
            "a clear of another collection, a copied column's label, leaves the finding",
            restated_rows().replace(
                "parent: parent.yaml\n",
                "parent: parent.yaml\nclear:\n  - columns.AGE.label\n",
            ),
            vec![row(
                9,
                5,
                "rows.scores.derivations",
                &format!(
                    "`NOTE` comes back from the parent; {}",
                    replace("rows.scores.derivations")
                ),
            )],
        ),
    ];
    for (label, text, expected) in cases {
        let directory = Scratch::new("lint-restatement");
        directory.write("parent.yaml", PARENT);
        directory.write("child.yaml", text);
        assert_eq!(rows(&directory, "child.yaml", false), expected, "{label}");
    }
}

/// An entry a grandparent supplies comes back through the parent, which
/// composes the chain above it (LAYER-3).
#[test]
fn an_entry_from_a_grandparent_comes_back_through_the_parent() {
    let directory = Scratch::new("lint-restatement-chain");
    directory.write("grandparent.yaml", PARENT);
    directory.write(
        "parent.yaml",
        "yamaa: \"1.0\"\ntype: spec\nparent: grandparent.yaml\n\nrows:\n  - id: scores\n    \
         derivations:\n      AVAL: SC.SCORE * 10\n",
    );
    directory.write(
        "child.yaml",
        child(
            "\nrows:\n  - id: scores\n    derivations:\n      USUBJID: SC.USUBJID\n      \
             AVAL: SC.SCORE * 10\n",
        ),
    );
    assert_eq!(
        rows(&directory, "child.yaml", false),
        [row(
            7,
            5,
            "rows.scores.derivations",
            "`VISIT` and `NOTE` come back from the parent; list `rows.scores.derivations` in \
             `clear` to replace the mapping, or write only what this layer changes",
        )]
    );
}

/// A fragment a layer includes composes before the layer's own fields, so an
/// entry the fragment supplies comes back too, and the message names what the
/// layer composes on (LAYER-3).
#[test]
fn an_entry_from_an_included_fragment_comes_back_through_the_include() {
    let restated = "\nrows:\n  - id: scores\n    derivations:\n      USUBJID: SC.USUBJID\n      \
                    VISIT: SC.VISIT\n      AVAL: SC.SCORE\n";
    let directory = Scratch::new("lint-restatement-include");
    directory.write("base.yaml", PARENT);
    directory.write(
        "fragment.yaml",
        "yamaa: \"1.0\"\ntype: spec\n\nrows:\n  - id: scores\n    derivations:\n      \
         FLAG: \"'Y'\"\n",
    );
    let message = |from: &str| {
        format!(
            "`NOTE` and `FLAG` come back from {from}; list `rows.scores.derivations` in \
             `clear` to replace the mapping, or write only what this layer changes"
        )
    };
    for (label, header, line, from) in [
        (
            "a parent and a fragment",
            "parent: base.yaml\ninclude: [fragment.yaml]\n",
            8,
            "the parent or an included fragment",
        ),
        (
            "a fragment alone",
            "include: [base.yaml, fragment.yaml]\n",
            7,
            "an included fragment",
        ),
    ] {
        directory.write(
            "child.yaml",
            format!("yamaa: \"1.0\"\ntype: spec\n{header}{restated}"),
        );
        assert_eq!(
            rows(&directory, "child.yaml", false),
            [row(line, 5, "rows.scores.derivations", &message(from))],
            "{label}"
        );
    }
}

/// Either meaning may be the author's, so `fix` leaves the file as written
/// and the finding remains; a suppression with a reason covers it.
#[test]
fn fix_leaves_a_partial_restatement_and_a_suppression_covers_it() {
    let directory = Scratch::new("lint-restatement-fix");
    directory.write("parent.yaml", PARENT);
    let path = directory.write("child.yaml", restated_rows());
    let file = path.to_string_lossy().into_owned();
    let fixed = lint(std::slice::from_ref(&file), true).unwrap();
    assert!(fixed.rewritten().is_empty(), "fix rewrites nothing");
    assert_eq!(
        fixed.summary(),
        "1 lint finding in 1 of 1 file; fixed 0 files"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), restated_rows());
    directory.write(
        "child.yaml",
        restated_rows().replace(
            "    derivations:\n",
            "    # yamaa-lint: allow partial_restatement -- NOTE keeps the parent's comment\n    \
             derivations:\n",
        ),
    );
    assert_eq!(rows(&directory, "child.yaml", false), []);
}

/// A chain the lint cannot follow gives no finding and no refusal: the
/// parent's own refusal belongs to validation (LAYER-2).
#[test]
fn a_parent_that_cannot_be_read_gives_no_finding() {
    let directory = Scratch::new("lint-restatement-missing");
    directory.write("child.yaml", restated_rows());
    assert_eq!(rows(&directory, "child.yaml", false), []);
}
