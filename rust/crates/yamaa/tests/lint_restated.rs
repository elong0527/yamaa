//! `restated_inherited_value` through the public [`yamaa::lint`] facade: a
//! value a layer writes exactly as it inherits it, a root field, a member, a
//! member's field, a mapping's entry or a window's setting, which composes
//! the same specification as the layer without it (LAYER-5, LAYER-6,
//! LAYER-9). The seam is the variety the shared `lint` host fixture's one
//! layered case leaves out: chains, fragments, cleared values, paths
//! respelled from another directory and a copy whose producer cannot be
//! read. Each expected row is written by hand: its position is the key of
//! the restated value in the child as written, or the first character of a
//! list member it writes whole, and its message is the style guide's,
//! naming each layer the inherited value was written in as the child's
//! directory spells it.
#![cfg(feature = "yaml")]

use crate::support::scratch::Scratch;

use yamaa::lint;

/// The parent every table case inherits: an input with field types, an
/// output, a named window, columns, one with metadata, a row template and
/// root metadata.
const PARENT: &str = r#"yamaa: "1.0"
type: spec
domain: ADTOY
keys: [USUBJID, VISIT]
input:
  SC:
    path: input/scores.csv
    types: {VISIT: int, SCORE: float}

output:
  path: out.csv
  columns: [USUBJID, VISIT, AVAL, NOTE]

windows:
  BYSUBJ:
    group_by: [SC.USUBJID]
    order_by: [SC.VISIT]

columns:
  - {name: USUBJID, type: str, label: Subject}
  - {name: VISIT, type: int, label: Visit}
  - {name: AVAL, type: float, label: Value, metadata: {origin: Collected}}
  - {name: NOTE, type: str, label: Note}

rows:
  - id: scores
    dataset: SC
    derivations:
      USUBJID: SC.USUBJID
      VISIT: SC.VISIT
      AVAL: SC.SCORE
      NOTE: SC.COMMENT

metadata: {owner: biostatistics, status: draft}
"#;

/// A child of `parent` whose sections are `body`.
fn layer(parent: &str, body: &str) -> String {
    format!("yamaa: \"1.0\"\ntype: spec\nparent: {parent}\n{body}")
}

/// A child of `parent.yaml` whose sections are `body`.
fn child(body: &str) -> String {
    layer("parent.yaml", body)
}

/// One row of the lint table: (line, column, finding, spec path, message).
type Row = (usize, usize, String, String, String);

/// The row of a value at `spec_path` that restates what `from` wrote.
fn restated(line: usize, column: usize, spec_path: &str, from: &str) -> Row {
    (
        line,
        column,
        "restated_inherited_value".to_owned(),
        spec_path.to_owned(),
        format!(
            "`{spec_path}` writes the value it inherits from {from}; remove it to keep inheriting"
        ),
    )
}

/// The rows `lint` gives the file `name` of `directory`, each of them in that
/// file.
fn rows(directory: &Scratch, name: &str) -> Vec<Row> {
    let file = directory.join(name).to_string_lossy().into_owned();
    let linted = lint(std::slice::from_ref(&file), false).unwrap();
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
fn a_value_written_as_it_is_inherited_is_reported_at_the_outermost_value() {
    let from = "parent.yaml";
    let cases: Vec<(&str, String, Vec<Row>)> = vec![
        (
            "a root scalar and a root list",
            child("domain: ADTOY\nkeys: [USUBJID, VISIT]\n"),
            vec![restated(4, 1, "domain", from), restated(5, 1, "keys", from)],
        ),
        (
            "a root field that replaces whole, written whole in another order",
            child("\nmetadata: {status: draft, owner: biostatistics}\n"),
            vec![restated(5, 1, "metadata", from)],
        ),
        (
            "a root field that replaces whole is compared whole, never by entry",
            child("\nmetadata: {owner: biostatistics}\n"),
            vec![],
        ),
        (
            "a whole member, its identity never reported alone",
            child("\ncolumns:\n  - {name: VISIT, type: int, label: Visit}\n"),
            vec![restated(6, 5, "columns.VISIT", from)],
        ),
        (
            "a member's field beside a change",
            child("\ncolumns:\n  - name: AVAL\n    type: float\n    label: Analysis Value\n"),
            vec![restated(7, 5, "columns.AVAL.type", from)],
        ),
        (
            "a member's whole mapping",
            child("\ncolumns:\n  - name: AVAL\n    metadata: {origin: Collected}\n"),
            vec![restated(7, 5, "columns.AVAL.metadata", from)],
        ),
        (
            "a mapping's entry beside an added one",
            child("\ncolumns:\n  - name: AVAL\n    metadata: {origin: Collected, unit: points}\n"),
            vec![restated(7, 16, "columns.AVAL.metadata.origin", from)],
        ),
        (
            "an input's field type, beside the type that comes back",
            child("input:\n  SC:\n    types: {VISIT: int}\n"),
            vec![
                (
                    6,
                    5,
                    "partial_restatement".to_owned(),
                    "input.SC.types".to_owned(),
                    "`SCORE` comes back from the parent; list `input.SC.types` in `clear` to \
                     replace the mapping, or write only what this layer changes"
                        .to_owned(),
                ),
                restated(6, 13, "input.SC.types.VISIT", from),
            ],
        ),
        (
            "a window's setting beside a change",
            child("\nwindows:\n  BYSUBJ:\n    group_by: [SC.USUBJID]\n    order_by: [SC.SCORE]\n"),
            vec![restated(7, 5, "windows.BYSUBJ.group_by", from)],
        ),
        (
            "a row template's derivations, one changed",
            child(
                "\nrows:\n  - id: scores\n    derivations:\n      USUBJID: SC.USUBJID\n      \
                 VISIT: SC.VISIT\n      AVAL: SC.SCORE * 2\n      NOTE: SC.COMMENT\n",
            ),
            vec![
                restated(8, 7, "rows.scores.derivations.USUBJID", from),
                restated(9, 7, "rows.scores.derivations.VISIT", from),
                restated(11, 7, "rows.scores.derivations.NOTE", from),
            ],
        ),
        (
            "a value that differs is a change",
            child("\ncolumns:\n  - name: NOTE\n    label: Comment\n"),
            vec![],
        ),
        (
            "a field the layer clears is written again, not restated",
            child(
                "clear:\n  - columns.VISIT.label\n\ncolumns:\n  - name: VISIT\n    label: Visit\n",
            ),
            vec![],
        ),
        (
            "an entry of a mapping the layer clears is written again, not restated",
            child(
                "clear:\n  - rows.scores.derivations\n\nrows:\n  - id: scores\n    \
                 derivations:\n      USUBJID: SC.USUBJID\n",
            ),
            vec![],
        ),
    ];
    for (label, text, expected) in cases {
        let directory = Scratch::new("lint-restated");
        directory.write("parent.yaml", PARENT);
        directory.write("child.yaml", text);
        assert_eq!(rows(&directory, "child.yaml"), expected, "{label}");
    }
}

/// Each restated value names the layers that wrote what it inherits, in
/// contribution order: a grandparent through the parent, the parent, and a
/// fragment the child includes, which composes before the child's own fields
/// (LAYER-3).
#[test]
fn a_restated_value_names_the_layers_it_inherits_from() {
    let directory = Scratch::new("lint-restated-chain");
    directory.write("grandparent.yaml", PARENT);
    directory.write(
        "parent.yaml",
        layer(
            "grandparent.yaml",
            "\ncolumns:\n  - name: VISIT\n    label: Visit Number\n",
        ),
    );
    directory.write(
        "fragment.yaml",
        "yamaa: \"1.0\"\ntype: spec\n\ncolumns:\n  - name: AVAL\n    label: Score\n",
    );
    directory.write(
        "child.yaml",
        layer(
            "parent.yaml",
            "include: [fragment.yaml]\n\ncolumns:\n  - name: USUBJID\n    label: Subject\n\n  \
             - {name: VISIT, type: int, label: Visit Number}\n\n  - name: AVAL\n    \
             label: Score\n",
        ),
    );
    assert_eq!(
        rows(&directory, "child.yaml"),
        [
            restated(8, 5, "columns.USUBJID.label", "grandparent.yaml"),
            restated(10, 5, "columns.VISIT", "grandparent.yaml and parent.yaml"),
            restated(13, 5, "columns.AVAL.label", "fragment.yaml"),
        ]
    );
}

/// A relative path is compared as the engine reads it, respelled from the
/// entry (LAYER-10): the parent's `input/scores.csv` names `../input/scores.csv`
/// from a child one directory down, so only that spelling restates it. The
/// child writes the input's shorthand, so the finding is at the input it
/// writes, and the message names the parent as the child's directory spells
/// it.
#[test]
fn a_path_restates_only_the_location_it_inherits() {
    let directory = Scratch::new("lint-restated-paths");
    directory.write("parent.yaml", PARENT);
    for (label, path, expected) in [
        ("another location", "input/scores.csv", vec![]),
        (
            "the inherited location",
            "../input/scores.csv",
            vec![restated(5, 3, "input.SC", "../parent.yaml")],
        ),
    ] {
        directory.write(
            "sub/child.yaml",
            layer("../parent.yaml", &format!("input:\n  SC: {path}\n")),
        );
        assert_eq!(rows(&directory, "sub/child.yaml"), expected, "{label}");
    }
}

/// A copy entry composes only once its producer is read (SPEC-9): a chain
/// whose producer cannot be read is not read for the finding, and validation
/// reports why.
#[test]
fn a_chain_whose_producer_cannot_be_read_gives_no_finding() {
    let directory = Scratch::new("lint-restated-producer");
    directory.write(
        "parent.yaml",
        PARENT
            .replace(
                "    types: {VISIT: int, SCORE: float}\n",
                "    types: {VISIT: int, SCORE: float}\n  ADSL: {path: adsl.csv, schema: adsl.yaml}\n",
            )
            .replace(
                "columns:\n  - {name: USUBJID",
                "columns:\n  - copy: {dataset: ADSL, columns: [AGE]}\n  - {name: USUBJID",
            ),
    );
    directory.write(
        "child.yaml",
        child("\ncolumns:\n  - name: VISIT\n    label: Visit\n"),
    );
    assert_eq!(rows(&directory, "child.yaml"), []);
}

/// Either meaning may be the author's, so `fix` leaves the file as written
/// and the finding remains; a suppression with a reason covers it.
#[test]
fn fix_leaves_a_restated_value_and_a_suppression_covers_it() {
    let directory = Scratch::new("lint-restated-fix");
    directory.write("parent.yaml", PARENT);
    let text = child("\ncolumns:\n  - name: VISIT\n    label: Visit\n");
    let path = directory.write("child.yaml", &text);
    let file = path.to_string_lossy().into_owned();
    let fixed = lint(std::slice::from_ref(&file), true).unwrap();
    assert!(fixed.rewritten().is_empty(), "fix rewrites nothing");
    assert_eq!(
        fixed.summary(),
        "1 lint finding in 1 of 1 file; fixed 0 files"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    directory.write(
        "child.yaml",
        text.replace(
            "  - name: VISIT\n",
            "  # yamaa-lint: allow restated_inherited_value -- the label follows the CRF\n  \
             - name: VISIT\n",
        ),
    );
    assert_eq!(rows(&directory, "child.yaml"), []);
}
