//! The lint's findings and fixes, case by case. Each case writes the departure
//! into a canonical specification and checks the finding, its position, its
//! spec path and message, and what the proved fixes write.

use super::{check_text, fix_text, Finding};

const CANONICAL: &str = r#"yamaa: "1.0"
type: spec
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  DM: input/dm.csv
  EX: input/ex.csv
base: DM

output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, TRTSDT]

intermediates:
  - id: FIRSTDOSE
    dataset: EX
    key: [STUDYID, USUBJID]
    order_by: [EX.EXSTDTC]
    keep: first

columns:
  # The study identifier as collected.
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: DM.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: DM.USUBJID

  - name: TRTSDT
    type: str
    label: Date of First Exposure
    derivation:
      case:
        - when: "FIRSTDOSE.EXSTDTC IS NULL"
          then: null
        - otherwise: FIRSTDOSE.EXSTDTC

verifications:
  - row_count: {min: 1}
  - unique: [USUBJID]
"#;

fn findings(text: &str) -> Vec<Finding> {
    check_text(text.as_bytes()).expect("the text is admitted")
}

fn names(text: &str) -> Vec<&'static str> {
    findings(text)
        .into_iter()
        .map(|item| item.finding)
        .collect()
}

fn paths(text: &str) -> Vec<(&'static str, String)> {
    findings(text)
        .into_iter()
        .map(|item| (item.finding, item.spec_path))
        .collect()
}

fn fixed(text: &str) -> String {
    String::from_utf8(fix_text(text.as_bytes()).expect("the text is admitted")).unwrap()
}

/// Fix `text`, requiring the fix to be proved and settled, and return the
/// fixed text with the findings that remain.
fn fix(text: &str) -> (String, Vec<&'static str>) {
    let after = fixed(text);
    assert!(super::fix::proved(text, &after));
    // A fix is kept only when fixing the fixed file changes nothing.
    assert_eq!(fixed(&after), after);
    let remaining = names(&after);
    (after, remaining)
}

/// CANONICAL with the blank line after the first column entry removed.
fn packed() -> String {
    CANONICAL.replace(
        "    derivation: DM.STUDYID\n\n",
        "    derivation: DM.STUDYID\n",
    )
}

#[test]
fn canonical_specification_has_no_findings() {
    assert!(findings(CANONICAL).is_empty());
}

#[test]
fn a_finding_carries_its_line_column_and_name() {
    let text = CANONICAL.replace("    dataset: EX\n", "    dataset: EX\n\n\n");
    let first = &findings(&text)[0];
    assert_eq!(first.finding, "blank_lines");
    assert_eq!((first.line, first.column), (18, 1));
    assert_eq!(first.message, "remove the second blank line");
}

#[test]
fn root_fields_follow_the_schema_order() {
    let text = CANONICAL
        .replace(
            "output:\n  path: adsl.csv\n  columns: [STUDYID, USUBJID, TRTSDT]\n\n",
            "",
        )
        .replace(
            "columns:\n  # The",
            "output:\n  path: adsl.csv\n  columns: [STUDYID, USUBJID, TRTSDT]\n\ncolumns:\n  # The",
        );
    let first = &findings(&text)[0];
    assert_eq!(
        (first.finding, first.spec_path.as_str()),
        ("field_order", "output")
    );
    assert_eq!(first.message, "write `output` before `intermediates`");
    assert_eq!(fix(&text), (CANONICAL.to_owned(), vec![]));
}

#[test]
fn parent_and_filter_belong_to_the_header() {
    let text = r#"yamaa: "1.0"
type: spec
domain: ADSL
parent: base.yaml
keys: [USUBJID]
input:
  DM: input/dm.csv

output:
  path: adsl.csv
  columns: [USUBJID]

columns:
  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: DM.USUBJID

filter: "DM.ARMCD IS NOT NULL"
"#;
    // One finding per mapping: the first field written out of order.
    assert_eq!(names(text), ["field_order"]);
    let (after, remaining) = fix(text);
    assert!(remaining.is_empty());
    assert!(after.starts_with(
        "yamaa: \"1.0\"\ntype: spec\nparent: base.yaml\ndomain: ADSL\nkeys: [USUBJID]\ninput:\n  \
         DM: input/dm.csv\nfilter: \"DM.ARMCD IS NOT NULL\"\n\noutput:\n"
    ));
}

#[test]
fn entry_fields_move_with_the_list_dash() {
    let text = CANONICAL
        .replace(
            "  - name: USUBJID\n    type: str\n",
            "  - type: str\n    name: USUBJID\n",
        )
        .replace(
            "    order_by: [EX.EXSTDTC]\n    keep: first\n",
            "    keep: first\n    order_by: [EX.EXSTDTC]\n",
        );
    assert_eq!(names(&text), ["field_order", "field_order"]);
    assert_eq!(fix(&text), (CANONICAL.to_owned(), vec![]));
}

#[test]
fn an_entry_with_a_blank_line_inside_is_not_reordered() {
    let text = CANONICAL.replace(
        "  - name: USUBJID\n    type: str\n",
        "  - type: str\n\n    name: USUBJID\n",
    );
    assert_eq!(fix(&text).1, ["field_order"]);
}

#[test]
fn multiline_entries_are_separated_by_one_blank_line() {
    let text = packed();
    let first = &findings(&text)[0];
    assert_eq!(
        (first.finding, first.spec_path.as_str(), first.line),
        ("entry_spacing", "columns.USUBJID", 27)
    );
    assert_eq!(
        first.message,
        "leave one blank line between `columns` entries"
    );
    assert_eq!(fix(&text), (CANONICAL.to_owned(), vec![]));
}

#[test]
fn the_blank_line_goes_above_the_entry_comment() {
    let text = CANONICAL.replace(
        "    keep: first\n\ncolumns:\n",
        "    keep: first\n  # The last dose.\n  - id: LASTDOSE\n    dataset: EX\n    \
         order_by: [EX.EXSTDTC]\n    keep: last\n\ncolumns:\n",
    );
    assert_eq!(names(&text), ["entry_spacing"]);
    assert!(fix(&text)
        .0
        .contains("    keep: first\n\n  # The last dose.\n  - id: LASTDOSE\n"));
}

#[test]
fn single_line_entries_may_stand_together() {
    let text = CANONICAL.replace(
        "columns:\n  # The study identifier as collected.\n",
        "columns:\n  - {name: SITEID, type: str, label: Site, derivation: DM.SITEID}\n  \
         - {name: ARM, type: str, label: Arm, derivation: DM.ARM}\n\n",
    );
    assert!(names(&text).is_empty());
}

#[test]
fn sections_need_a_blank_line_and_the_header_does_not() {
    let text = CANONICAL.replace("base: DM\n\noutput:", "base: DM\noutput:");
    let first = &findings(&text)[0];
    assert_eq!(
        (first.finding, first.spec_path.as_str()),
        ("root_spacing", "output")
    );
    assert_eq!(first.message, "leave one blank line before `output`");
    assert_eq!(fix(&text).0, CANONICAL);
    let spaced = CANONICAL.replace("domain: ADSL\n", "\ndomain: ADSL\n\n");
    assert!(names(&spaced).is_empty());
}

/// A 1.0 specification layer in canonical form: it declares the 1.x header,
/// and writes `clear`, `model`, an input's `byte_order_mark` and `key` and an
/// intermediate's `absent`, which only the 1.0 schema orders.
const VERSIONED: &str = r#"yamaa: "1.0"
type: spec
model: ADaM
parent: base.yaml
clear:
  - columns.TRTSDT.derivation
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  DM:
    path: input/dm.csv
    byte_order_mark: true
    key: [STUDYID, USUBJID]
  EX: input/ex.csv
base: DM

output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, TRTSDT]

intermediates:
  - id: FIRSTDOSE
    dataset: EX
    key: [STUDYID, USUBJID]
    order_by: [EX.EXSTDTC]
    keep: first
    absent: NONE

columns:
  - name: TRTSDT
    type: str
    label: Date of First Exposure
    derivation: FIRSTDOSE.EXSTDTC
"#;

#[test]
fn a_1_0_header_stands_together_before_parent_and_domain() {
    assert!(findings(VERSIONED).is_empty());
    // As the 0.5 header may, the fields before `output` may also stand apart,
    // so a fix leaves such a header as written.
    let spaced = VERSIONED.replace("type: spec\nmodel: ADaM\n", "\ntype: spec\n\nmodel: ADaM\n");
    assert!(names(&spaced).is_empty());
    assert_eq!(fixed(&spaced), spaced);
    let text = VERSIONED.replace("base: DM\n\noutput:", "base: DM\noutput:");
    assert_eq!(
        paths(&text),
        [("root_spacing", "output".to_owned())],
        "`output` still opens a section"
    );
    assert_eq!(fix(&text), (VERSIONED.to_owned(), vec![]));
}

#[test]
fn a_1_0_specification_follows_the_1_0_field_order() {
    for (text, spec_path, message) in [
        (
            VERSIONED
                .replace("model: ADaM\n", "")
                .replace("domain: ADSL\n", "domain: ADSL\nmodel: ADaM\n"),
            "model",
            "write `model` before `domain`",
        ),
        (
            VERSIONED
                .replace("clear:\n  - columns.TRTSDT.derivation\n", "")
                .replace(
                    "keys: [STUDYID, USUBJID]\n",
                    "keys: [STUDYID, USUBJID]\nclear:\n  - columns.TRTSDT.derivation\n",
                ),
            "clear",
            "write `clear` before `keys`",
        ),
        (
            VERSIONED
                .replace(
                    "yamaa: \"1.0\"\ntype: spec\nmodel: ADaM\n",
                    "yamaa: \"1.0\"\n",
                )
                .replace("base: DM\n", "base: DM\ntype: spec\nmodel: ADaM\n"),
            "type",
            "write `type` before `base`",
        ),
        (
            VERSIONED.replace(
                "    byte_order_mark: true\n    key: [STUDYID, USUBJID]\n",
                "    key: [STUDYID, USUBJID]\n    byte_order_mark: true\n",
            ),
            "input.DM.byte_order_mark",
            "write `byte_order_mark` before `key`",
        ),
        (
            VERSIONED.replace(
                "    keep: first\n    absent: NONE\n",
                "    absent: NONE\n    keep: first\n",
            ),
            "intermediates.FIRSTDOSE.keep",
            "write `keep` before `absent`",
        ),
    ] {
        let found = findings(&text);
        assert_eq!(
            found
                .iter()
                .map(|item| (item.finding, item.spec_path.as_str(), item.message.as_str()))
                .collect::<Vec<_>>(),
            [("field_order", spec_path, message)]
        );
        // The fix moves the fields back, the 1.0 header together.
        assert_eq!(fix(&text), (VERSIONED.to_owned(), vec![]));
    }
}

/// A 1.0 project file in canonical form: its header together, then one blank
/// line before each submission section.
const PROJECT: &str = r#"yamaa: "1.0"
type: project
functions: [functions/bmi.yaml]
environments:
  python: {lock: uv.lock}
codelists:
  - standard: {name: CDISC/NCI, publishing_set: SDTM, version: "2023-12-15"}
    codelists:
      - id: SEX
        name: Sex
        items: [{value: F}, {value: M}]
study:
  id: STUDY01
  name: STUDY01
  description: Example study
  protocol_name: STUDY-01

sdtm:
  specs: [sdtm/dm.yaml]
  standards:
    - {name: SDTMIG, version: "3.4", status: Final}
  define: sdtm/define.xml

adam:
  specs: [adam/adsl.yaml]
"#;

/// A 1.0 function file in canonical form: one blank line before `tests` and
/// between its entries.
const FUNCTION: &str = r#"yamaa: "1.0"
type: function
name: bmi
description: Body mass index from kilograms and centimetres.
params:
  - {name: weight_kg, type: float}
  - {name: height_cm, type: float}
returns: float
implementations: {python: projectbmi.bmi, r: projectbmi::bmi}

tests:
  - id: normal
    covers: [normal, numeric-comparison]
    args: {weight_kg: 70.0, height_cm: 175.0}
    result: 22.857142857142858

  - id: zero-weight
    covers: [boundary]
    args: {weight_kg: 0.0, height_cm: 175.0}
    result: 0.0
"#;

/// A 1.0 codelists file in canonical form: one blank line before `codelists`
/// and between its entries.
const CODELISTS: &str = r#"yamaa: "1.0"
type: codelists
standard: {name: CDISC/NCI, publishing_set: SDTM, version: "2023-12-15"}

codelists:
  - id: SEX
    name: Sex
    items:
      - value: F
        decode: Female
      - value: M
        decode: Male

  - id: AGEU
    name: Age Unit
    items: [{value: YEARS}]
"#;

/// Each departure written into a canonical file: its finding, spec path and
/// message, and the fix that writes the canonical file back.
fn departures(canonical: &str, cases: &[(String, &str, &str, &str)]) {
    assert!(findings(canonical).is_empty(), "{canonical}");
    for (text, finding, spec_path, message) in cases {
        let found = findings(text);
        assert_eq!(
            found
                .iter()
                .map(|item| (item.finding, item.spec_path.as_str(), item.message.as_str()))
                .collect::<Vec<_>>(),
            [(*finding, *spec_path, *message)],
            "{text}"
        );
        assert_eq!(fix(text), (canonical.to_owned(), vec![]), "{text}");
    }
}

#[test]
fn a_1_0_project_file_follows_the_project_schema() {
    let study = "study:\n  id: STUDY01\n  name: STUDY01\n  description: Example study\n  \
                 protocol_name: STUDY-01\n";
    departures(
        PROJECT,
        &[
            (
                PROJECT
                    .replace(study, "")
                    .replace("codelists:\n  - ", &format!("{study}codelists:\n  - ")),
                "field_order",
                "codelists",
                "write `codelists` before `study`",
            ),
            (
                PROJECT.replace(
                    "  specs: [sdtm/dm.yaml]\n  standards:\n    - {name: SDTMIG, version: \"3.4\", status: Final}\n",
                    "  standards:\n    - {name: SDTMIG, version: \"3.4\", status: Final}\n  specs: [sdtm/dm.yaml]\n",
                ),
                "field_order",
                "sdtm.specs",
                "write `specs` before `standards`",
            ),
            (
                PROJECT.replace(
                    "        name: Sex\n        items: [{value: F}, {value: M}]\n",
                    "        items: [{value: F}, {value: M}]\n        name: Sex\n",
                ),
                "field_order",
                "codelists[0].codelists.Sex.name",
                "write `name` before `items`",
            ),
            (
                PROJECT.replace("define.xml\n\nadam:", "define.xml\nadam:"),
                "root_spacing",
                "adam",
                "leave one blank line before `adam`",
            ),
        ],
    );
}

#[test]
fn a_1_0_function_file_follows_the_function_schema() {
    departures(
        FUNCTION,
        &[
            (
                FUNCTION
                    .replace("returns: float\n", "")
                    .replace("description: Body", "returns: float\ndescription: Body"),
                "field_order",
                "description",
                "write `description` before `returns`",
            ),
            (
                FUNCTION.replace(
                    "    args: {weight_kg: 70.0, height_cm: 175.0}\n    result: 22.857142857142858\n",
                    "    result: 22.857142857142858\n    args: {weight_kg: 70.0, height_cm: 175.0}\n",
                ),
                "field_order",
                "tests.normal.args",
                "write `args` before `result`",
            ),
            (
                FUNCTION.replace("22.857142857142858\n\n  - id:", "22.857142857142858\n  - id:"),
                "entry_spacing",
                "tests.zero-weight",
                "leave one blank line between `tests` entries",
            ),
            (
                FUNCTION.replace("projectbmi::bmi}\n\ntests:", "projectbmi::bmi}\ntests:"),
                "root_spacing",
                "tests",
                "leave one blank line before `tests`",
            ),
        ],
    );
}

#[test]
fn a_1_0_codelists_file_follows_the_codelists_schema() {
    departures(
        CODELISTS,
        &[
            (
                CODELISTS.replace(
                    "  - id: SEX\n    name: Sex\n",
                    "  - name: Sex\n    id: SEX\n",
                ),
                "field_order",
                "codelists.Sex.id",
                "write `id` before `name`",
            ),
            (
                CODELISTS.replace(
                    "      - value: M\n        decode: Male\n",
                    "      - decode: Male\n        value: M\n",
                ),
                "field_order",
                "codelists.Sex.items[1].value",
                "write `value` before `decode`",
            ),
            (
                CODELISTS.replace("decode: Male\n\n  - id:", "decode: Male\n  - id:"),
                "entry_spacing",
                "codelists['Age Unit']",
                "leave one blank line between `codelists` entries",
            ),
        ],
    );
    // The layout every file shares is read as well.
    let doubled = CODELISTS.replacen('\n', "\n\n\n", 1);
    assert_eq!(names(&doubled), ["blank_lines"]);
    assert_eq!(fix(&doubled), (CODELISTS.replacen('\n', "\n\n", 1), vec![]));
}

#[test]
fn blank_lines_are_reported_and_removed() {
    for (text, message) in [
        (format!("\n{CANONICAL}"), "remove the leading blank line"),
        (format!("{CANONICAL}\n"), "remove the blank line at the end"),
        (
            CANONICAL.trim_end_matches('\n').to_owned(),
            "end the file with a line break",
        ),
        (
            CANONICAL.replace("columns:\n  # The", "columns:\n\n  # The"),
            "remove the blank line after the key that opens this block",
        ),
        (
            CANONICAL.replace("\ncolumns:\n", "\n\ncolumns:\n"),
            "remove the second blank line",
        ),
    ] {
        let found = findings(&text)
            .into_iter()
            .map(|item| (item.finding, item.message))
            .collect::<Vec<_>>();
        assert_eq!(found, [("blank_lines", message.to_owned())], "{message}");
        assert_eq!(fix(&text), (CANONICAL.to_owned(), vec![]), "{message}");
    }
}

#[test]
fn a_missing_line_break_is_reported_at_the_end_of_the_last_line() {
    let text = CANONICAL.trim_end_matches('\n');
    let lines = text.lines().collect::<Vec<_>>();
    let found = findings(text)
        .into_iter()
        .map(|item| (item.finding, item.line, item.column))
        .collect::<Vec<_>>();
    assert_eq!(
        found,
        [("blank_lines", lines.len(), lines[lines.len() - 1].len() + 1)]
    );
}

#[test]
fn a_fix_keeps_crlf_line_endings() {
    let text = CANONICAL
        .replace("\n\noutput:", "\noutput:")
        .replace('\n', "\r\n");
    assert_eq!(names(&text), ["root_spacing"]);
    let (after, remaining) = fix(&text);
    assert!(remaining.is_empty());
    assert_eq!(after.matches('\n').count(), after.matches("\r\n").count());
    assert_eq!(after.replace("\r\n", "\n"), CANONICAL);
}

#[test]
fn a_fix_keeps_the_dominant_ending_of_a_mixed_file() {
    let broken = CANONICAL.replace("\n\noutput:", "\noutput:");
    let lines = broken.split('\n').collect::<Vec<_>>();
    let mixed_lf = format!("{}\r\n{}", lines[..3].join("\n"), lines[3..].join("\n"));
    assert_eq!(fix(&mixed_lf), (CANONICAL.to_owned(), vec![]));
    let mixed_crlf = broken.replace('\n', "\r\n").replacen("\r\n", "\n", 1);
    let (after, remaining) = fix(&mixed_crlf);
    assert!(remaining.is_empty());
    assert_eq!(after.matches('\n').count(), after.matches("\r\n").count());
    assert_eq!(after.replace("\r\n", "\n"), CANONICAL);
}

#[test]
fn a_crlf_line_is_measured_without_its_ending() {
    let label = "    label: Study Identifier\n";
    for (width, expected) in [(79, vec![]), (80, vec!["line is 80 characters"])] {
        let text = CANONICAL
            .replace(label, &format!("    label: {}\n", "x".repeat(width - 11)))
            .replace('\n', "\r\n");
        let found = findings(&text)
            .into_iter()
            .filter(|item| item.finding == "line_width")
            .map(|item| item.message.split(';').next().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(found, expected);
    }
}

#[test]
fn lone_cr_endings_are_lines_and_a_final_break() {
    let text = CANONICAL
        .replace("\n\noutput:", "\noutput:")
        .replace('\n', "\r");
    assert_eq!(names(&text), ["root_spacing"]);
    let (after, remaining) = fix(&text);
    assert!(remaining.is_empty());
    assert!(!after.contains('\n'));
    assert_eq!(after.replace('\r', "\n"), CANONICAL);
}

#[test]
fn blank_lines_inside_a_block_scalar_are_value_text() {
    let text = CANONICAL.replace(
        "    derivation: DM.STUDYID\n",
        "    derivation: DM.STUDYID\n    metadata:\n      note: |\n        First paragraph.\n\n\n        \
         Second paragraph.\n",
    );
    assert!(names(&text).is_empty());
}

#[test]
fn a_block_scalar_ends_on_its_last_line_under_every_ending() {
    let text = "yamaa: \"1.0\"\ntype: spec\nname: demo\nnotes: |\n  first\n\n  # not a comment\n  last\nafter: 1\n";
    let positions = |text: &str| {
        findings(text)
            .into_iter()
            .map(|item| (item.finding, item.line, item.spec_path))
            .collect::<Vec<_>>()
    };
    let expected = positions(text);
    assert!(!expected.is_empty());
    let lf_fixed = fixed(text);
    for ending in ["\r\n", "\r"] {
        let written = text.replace('\n', ending);
        assert_eq!(positions(&written), expected, "{ending:?}");
        let (after, remaining) = fix(&written);
        assert!(remaining.is_empty());
        assert_eq!(after.replace(ending, "\n"), lf_fixed);
    }
}

#[test]
fn a_fix_that_would_change_a_value_is_refused() {
    // The blank line the fix would insert before USUBJID becomes part of the
    // `|+` scalar, which keeps its trailing line breaks, so the proof fails.
    let text = CANONICAL.replace(
        "    derivation: DM.STUDYID\n\n",
        "    derivation: DM.STUDYID\n    metadata:\n      note: |+\n        kept\n",
    );
    assert_eq!(names(&text), ["entry_spacing"]);
    assert_eq!(fix(&text), (text.clone(), vec!["entry_spacing"]));
}

#[test]
fn long_flow_lists_wrap_after_the_bracket() {
    let columns = (0..12)
        .map(|index| format!("COLUMN{index:02}"))
        .collect::<Vec<_>>()
        .join(", ");
    let text = CANONICAL.replace(
        "  columns: [STUDYID, USUBJID, TRTSDT]",
        &format!("  columns: [STUDYID, USUBJID, TRTSDT, {columns}]"),
    );
    assert_eq!(names(&text), ["line_width"]);
    let (after, remaining) = fix(&text);
    assert!(remaining.is_empty());
    assert!(after.contains(
        "  columns: [STUDYID, USUBJID, TRTSDT, COLUMN00, COLUMN01, COLUMN02, COLUMN03,\n            \
         COLUMN04, COLUMN05, COLUMN06, COLUMN07, COLUMN08, COLUMN09,\n            COLUMN10, COLUMN11]\n"
    ));
}

#[test]
fn long_predicates_continue_after_a_comma_or_before_a_connective() {
    let predicate = "FIRSTDOSE.EXSTDTC IS NULL AND FIRSTDOSE.EXTRT IN ('PLACEBO', 'DRUG A') AND \
                     FIRSTDOSE.EXDOSE > 0";
    let text = CANONICAL.replace("\"FIRSTDOSE.EXSTDTC IS NULL\"", &format!("\"{predicate}\""));
    assert_eq!(names(&text), ["line_width"]);
    let (after, remaining) = fix(&text);
    assert!(remaining.is_empty());
    assert!(after.contains(
        "        - when: \"FIRSTDOSE.EXSTDTC IS NULL AND FIRSTDOSE.EXTRT IN ('PLACEBO',\n            \
         'DRUG A') AND FIRSTDOSE.EXDOSE > 0\"\n"
    ));
    let spaced =
        "FIRSTDOSE.EXSTDTC IS NULL AND FIRSTDOSE.EXTRT = 'PLACEBO' AND FIRSTDOSE.EXDOSE > 0 \
                  AND FIRSTDOSE.EXDOSU = 'mg'";
    let text = CANONICAL.replace("\"FIRSTDOSE.EXSTDTC IS NULL\"", &format!("\"{spaced}\""));
    let (after, remaining) = fix(&text);
    assert!(remaining.is_empty());
    assert!(after.contains(
        "        - when: \"FIRSTDOSE.EXSTDTC IS NULL AND FIRSTDOSE.EXTRT = 'PLACEBO'\n            \
         AND FIRSTDOSE.EXDOSE > 0 AND FIRSTDOSE.EXDOSU = 'mg'\"\n"
    ));
}

#[test]
fn long_flow_mappings_become_blocks_but_literals_stay() {
    let derivation =
        "{impute: {type: date, source: FIRSTDOSE.EXSTDTC, month: 1, day: last, invalid: null}}";
    let text = CANONICAL.replace(
        "    derivation: DM.USUBJID\n",
        &format!("    derivation: {derivation}\n"),
    );
    let (after, remaining) = fix(&text);
    assert!(remaining.is_empty());
    assert!(
        after.contains("    derivation:\n      impute: {type: date, source: FIRSTDOSE.EXSTDTC,")
            || after.contains(
                "    derivation:\n      impute:\n        type: date\n        source: FIRSTDOSE.EXSTDTC\n"
            )
    );
    assert!(after.contains("then: null"));
}

#[test]
fn a_long_comment_is_reported_and_left_to_its_author() {
    let text = CANONICAL.replace(
        "  # The study identifier as collected.",
        "  # The study identifier as collected, copied without change from the demographics domain.",
    );
    assert_eq!(fix(&text).1, ["line_width"]);
}

#[test]
fn the_line_width_boundary_is_seventy_nine() {
    let at_limit = CANONICAL.replace(
        "    label: Study Identifier\n",
        &format!("    label: {}\n", "x".repeat(68)),
    );
    assert!(names(&at_limit).is_empty());
    let over_limit = CANONICAL.replace(
        "    label: Study Identifier\n",
        &format!("    label: {}\n", "x".repeat(69)),
    );
    let label_line = over_limit
        .lines()
        .position(|line| line.contains("label:"))
        .unwrap()
        + 1;
    let found = findings(&over_limit)
        .into_iter()
        .map(|item| (item.finding, item.line, item.column, item.spec_path))
        .collect::<Vec<_>>();
    assert_eq!(
        found,
        [(
            "line_width",
            label_line,
            80,
            "columns.STUDYID.label".to_owned()
        )]
    );
}

#[test]
fn a_long_block_scalar_at_the_end_is_left_to_its_author() {
    // A block scalar's end sits on the line after the file's last written
    // line; wrapping must refuse the edit, not read past the end.
    let text = format!(
        "{CANONICAL}\nmetadata:\n  note: |\n    {}\n",
        "x".repeat(100)
    );
    assert_eq!(names(&text), ["line_width"]);
    assert_eq!(fix(&text), (text.clone(), vec!["line_width"]));
}

#[test]
fn a_suppression_with_a_reason_covers_its_entry() {
    let allowed = packed().replace(
        "  - name: USUBJID",
        "  # yamaa-lint: allow entry_spacing -- kept beside STUDYID\n  - name: USUBJID",
    );
    assert!(names(&allowed).is_empty());
    assert_eq!(fix(&allowed).0, allowed);
}

#[test]
fn an_invalid_suppression_is_reported_and_covers_nothing() {
    for (comment, message) in [
        (
            "# yamaa-lint: allow entry_spacing",
            "name the findings and give a reason after `--`",
        ),
        (
            "# yamaa-lint: allow entry_spacing --",
            "name the findings and give a reason after `--`",
        ),
        (
            "# yamaa-lint: allow spacing -- no such finding",
            "unknown finding `spacing`",
        ),
        (
            "# yamaa-lint: allow invalid_suppression -- cannot be suppressed",
            "unknown finding `invalid_suppression`",
        ),
        (
            "# yamaa-lint: allow literal_form -- retired with {literal: X}",
            "unknown finding `literal_form`",
        ),
        (
            "# yamaa-lint: ignore entry_spacing -- wrong verb",
            "name the findings and give a reason after `--`",
        ),
    ] {
        let text = packed().replace(
            "  - name: USUBJID",
            &format!("  {comment}\n  - name: USUBJID"),
        );
        let found = findings(&text);
        assert_eq!(
            found.iter().map(|item| item.finding).collect::<Vec<_>>(),
            ["invalid_suppression", "entry_spacing"],
            "{comment}"
        );
        assert_eq!(found[0].message, message);
        assert_eq!((found[0].column, found[0].spec_path.as_str()), (3, "$"));
    }
}

#[test]
fn a_suppression_names_several_findings_and_covers_the_whole_entry() {
    let text = packed()
        .replace(
            "  - name: USUBJID",
            "  # yamaa-lint: allow entry_spacing, line_width -- reviewed\n  - name: USUBJID",
        )
        .replace(
            "    label: Unique Subject Identifier\n",
            &format!("    label: Unique Subject Identifier {}\n", "x".repeat(60)),
        );
    assert!(names(&text).is_empty());
    // The old spelling of the comment is only a comment.
    let old = packed().replace(
        "  - name: USUBJID",
        "  # yamaa-style: allow entry_spacing -- kept beside STUDYID\n  - name: USUBJID",
    );
    assert_eq!(names(&old), ["entry_spacing"]);
}

#[test]
fn the_proof_keeps_types_and_comments_apart() {
    use super::fix::proved;
    assert!(proved("a: 1\nb: x\n", "b: x\na: 1\n"));
    for (before, after) in [
        ("a: 1\nb: [true, null]\n", "b: [true, null]\na: 1\n"),
        ("a: 01\n", "a: 1\n"),
        ("a: .nan\n", "a: null\n"),
    ] {
        assert!(proved(before, after), "{before} {after}");
    }
    for (before, after) in [
        ("a: 1\n", "a: \"1\"\n"),
        ("a: 1\n", "a: 1.0\n"),
        ("a: true\n", "a: 1\n"),
        ("# note\na: 1\n", "a: 1\n"),
        ("a: [1, 2]\n", "a: [2, 1]\n"),
        ("a: 9223372036854775808\n", "a: 9223372036854775809\n"),
        ("a: x\n", "a: x\na: x\n"),
        ("a: &x 1\n", "a: &x 1\n"),
        ("a: [\n", "a: [\n"),
        ("a: x # retained\n", "a: x\n"),
    ] {
        assert!(!proved(before, after), "{before} {after}");
    }
    // A large unchanged document still passes the shared admission.
    let source = (0..1024)
        .map(|index| format!("K{index}: {index}\n"))
        .collect::<String>();
    assert!(proved(&source, &source));
}

#[test]
fn text_that_admission_refuses_is_not_linted() {
    for text in ["a: [\n", "a: \u{e9}\n", "a: 1\na: 2\n", "a: &x 1\n"] {
        assert!(check_text(text.as_bytes()).is_err(), "{text}");
        assert!(fix_text(text.as_bytes()).is_err(), "{text}");
    }
}

/// A 1.0 specification whose two row templates derive the same three
/// columns, none of which has a column-level derivation.
const REPEATED: &str = r#"yamaa: "1.0"
type: spec
domain: DM
keys: [STUDYID, USUBJID]
input:
  DM: input/dm.csv

output:
  path: dm.csv
  columns: [STUDYID, USUBJID, DOMAIN]

columns:
  - name: STUDYID
    type: str
    label: Study Identifier

  - name: USUBJID
    type: str
    label: Unique Subject Identifier

  - name: DOMAIN
    type: str
    label: Domain Abbreviation

rows:
  - id: a
    derivations:
      STUDYID: DM.STUDYID
      USUBJID: DM.USUBJID
      DOMAIN: "'DM'"

  - id: b
    derivations:
      STUDYID: DM.STUDYID
      USUBJID: DM.USUBJID
      DOMAIN: "'DM'"
"#;

/// The finding for `column`, at the first template's derivation, which the
/// spec path notation names by the template's `id` (P-CLR-2).
fn repeated(column: &str) -> (&'static str, String) {
    (
        "repeated_row_derivation",
        format!("rows.a.derivations.{column}"),
    )
}

#[test]
fn repeated_row_derivations_move_to_column_level() {
    let found = findings(REPEATED);
    assert_eq!(
        found
            .iter()
            .map(|item| (item.finding, item.spec_path.clone()))
            .collect::<Vec<_>>(),
        [repeated("STUDYID"), repeated("USUBJID"), repeated("DOMAIN")]
    );
    assert_eq!((found[0].line, found[0].column), (28, 7));
    assert_eq!(
        found[0].message,
        "derivation of `STUDYID` is identical in every row template; move it to columns[].derivation"
    );
    // Authoring lints rewrite the value, so no fix is offered.
    assert_eq!(fixed(REPEATED), REPEATED);
}

#[test]
fn repeated_row_derivation_positions_ignore_stray_row_entries() {
    // A template is named by its `id`, wherever a stray entry stands.
    let stray = REPEATED.replace("rows:\n  - id: a\n", "rows:\n  - stray\n\n  - id: a\n");
    let expected = ["STUDYID", "USUBJID", "DOMAIN"].map(repeated);
    assert_eq!(paths(&stray), expected);
}

#[test]
fn repeated_row_derivation_needs_every_template() {
    let partial = REPEATED.replace(
        "  - id: b\n    derivations:\n      STUDYID: DM.STUDYID\n",
        "  - id: b\n    derivations:\n",
    );
    let found = paths(&partial);
    assert!(!found.contains(&repeated("STUDYID")));
    assert!(found.contains(&repeated("USUBJID")));
    assert!(found.contains(&repeated("DOMAIN")));
}

#[test]
fn repeated_row_derivation_skips_row_phase_work() {
    let aggregate = REPEATED
        .replace(
            "      DOMAIN: \"'DM'\"\n\n  - id: b",
            "      DOMAIN:\n        aggregate:\n          filter: \"DM.X = 1\"\n          expr: \"ONLY(DM.Y)\"\n\n  - id: b",
        )
        .replace(
            "      DOMAIN: \"'DM'\"\n",
            "      DOMAIN:\n        aggregate:\n          filter: \"DM.X = 1\"\n          expr: \"ONLY(DM.Y)\"\n",
        );
    let found = paths(&aggregate);
    assert!(found.contains(&repeated("STUDYID")));
    assert!(!found.contains(&repeated("DOMAIN")));
}

#[test]
fn repeated_row_derivation_skips_existing_defaults() {
    let defaulted = REPEATED.replace(
        "  - name: DOMAIN\n    type: str\n    label: Domain Abbreviation\n",
        "  - name: DOMAIN\n    type: str\n    label: Domain Abbreviation\n    derivation: \"'DM'\"\n",
    );
    let found = paths(&defaulted);
    assert!(!found.contains(&repeated("DOMAIN")));
    assert!(found.contains(&repeated("STUDYID")));
}

#[test]
fn repeated_row_derivation_skips_mixed_drivers_except_literals() {
    let mixed = REPEATED
        .replace(
            "  - id: b\n    derivations:",
            "  - id: b\n    dataset: OTHER\n    derivations:",
        )
        .replace(
            "input:\n  DM: input/dm.csv",
            "input:\n  DM: input/dm.csv\n  OTHER: input/other.csv",
        );
    let found = paths(&mixed);
    // A source read binds different records per driver, so it stays; a
    // literal is driver-independent and is still reported.
    assert!(!found.contains(&repeated("STUDYID")));
    assert!(found.contains(&repeated("DOMAIN")));
}

#[test]
fn repeated_row_derivation_skips_columns_read_by_a_filter() {
    let filtered = REPEATED
        .replace(
            "  - id: a\n    derivations:",
            "  - id: a\n    filter: \"DOMAIN = 'DM'\"\n    derivations:",
        )
        .replace(
            "  - id: b\n    derivations:",
            "  - id: b\n    filter: \"DOMAIN = 'DM'\"\n    derivations:",
        );
    let found = paths(&filtered);
    assert!(!found.contains(&repeated("DOMAIN")));
    assert!(found.contains(&repeated("STUDYID")));
    // A filter name with a leading underscore retains its derivation too.
    let underscored = REPEATED
        .replace("DOMAIN", "_X")
        .replace(
            "  - id: a\n    derivations:",
            "  - id: a\n    filter: \"_X = 'DM'\"\n    derivations:",
        )
        .replace(
            "  - id: b\n    derivations:",
            "  - id: b\n    filter: \"_X = 'DM'\"\n    derivations:",
        );
    assert!(!paths(&underscored).contains(&repeated("_X")));
}

const PREDICATE_CHAIN: &str = r#"yamaa: "1.0"
type: spec
domain: ADXX
keys: [STUDYID, USUBJID]
input:
  DM: input/dm.csv

output:
  path: adxx.csv
  columns: [STUDYID, USUBJID, RESULT]

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: DM.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: DM.USUBJID

  - name: AGG
    type: float
    label: Aggregate Value
    derivation:
      aggregate:
        filter: "DM.X = 1"
        expr: "ONLY(DM.Y)"

  - name: FLG
    type: str
    label: Aggregate Flag
    derivation: {flag: "AGG > 0"}

  - name: RESULT
    type: str
    label: Result

rows:
  - id: a
    derivations:
      RESULT: {flag: "FLG = 'Y'"}

  - id: b
    derivations:
      RESULT: {flag: "FLG = 'Y'"}
"#;

#[test]
fn repeated_row_derivation_follows_predicate_reads_of_blocked_columns() {
    // FLG reads the dataset-level AGG inside a flag string, so FLG is not
    // row-local either, and RESULT, which reads FLG, stays in its rows.
    assert!(findings(PREDICATE_CHAIN).is_empty());
    // Over row-local columns, the same predicate forms stay reported.
    let local = PREDICATE_CHAIN.replace(
        "    derivation:\n      aggregate:\n        filter: \"DM.X = 1\"\n        expr: \"ONLY(DM.Y)\"\n",
        "    derivation: DM.AGG\n",
    );
    assert!(paths(&local).contains(&repeated("RESULT")));
}

#[test]
fn a_redundant_intermediate_is_not_read_by_a_longer_name() {
    // The only qualified read is of the ADSL dataset, whose name merely
    // ends with the intermediate's id.
    let suffix = REDUNDANT_1_0
        .replace("  - id: ADSL1\n", "  - id: DSL\n")
        .replace("    derivation: ADSL1.AGE", "    derivation: ADSL.AGE");
    assert!(!names(&suffix).contains(&"redundant_intermediate"));
    let genuine = suffix.replace("    derivation: ADSL.AGE", "    derivation: DSL.AGE");
    assert!(names(&genuine).contains(&"redundant_intermediate"));
    // A read of a `SELF` intermediate is not row-local (SPEC-6); a read of a
    // dataset whose name ends with its id is no read of it.
    let self_read = |derivation: &str| {
        REPEATED
            .replace(
                "input:\n  DM: input/dm.csv",
                "input:\n  DM: input/dm.csv\n  ADSL: input/adsl.csv",
            )
            .replace(
                "output:\n  path: dm.csv",
                "intermediates:\n  - id: DSL\n    dataset: SELF\n    key: [STUDYID, USUBJID]\n\n\
                 output:\n  path: dm.csv",
            )
            .replace("DOMAIN: \"'DM'\"", &format!("DOMAIN: {derivation}"))
    };
    assert!(paths(&self_read("ADSL.AGE")).contains(&repeated("DOMAIN")));
    assert!(!paths(&self_read("DSL.AGE")).contains(&repeated("DOMAIN")));
}

#[test]
fn qualified_reads_after_escaped_unicode_preserve_identifier_boundaries() {
    // ASCII YAML source can decode to two-, three- or four-byte UTF-8 scalars.
    for escape in [r"\u00e9", r"\u4e2d", r"\U0001f600"] {
        let redundant = REDUNDANT_1_0.replace(
            "derivation: ADSL1.AGE",
            &format!("derivation: \"{escape} ADSL1.AGE\""),
        );
        assert_eq!(paths(&redundant), paths(REDUNDANT_1_0));
        assert!(!names(&redundant.replace(" ADSL1.AGE", " XADSL1.AGE"))
            .contains(&"redundant_intermediate"));

        // A read of a `SELF` intermediate is not row-local (SPEC-6).
        let repeated_read = REPEATED
            .replace(
                "columns:\n  - name:",
                "intermediates:\n  - id: PICK\n    dataset: SELF\n\ncolumns:\n  - name:",
            )
            .replace("DOMAIN: \"'DM'\"", &format!("DOMAIN: \"{escape} PICK.X\""));
        assert!(!paths(&repeated_read).contains(&repeated("DOMAIN")));
        assert!(paths(&repeated_read.replace(" PICK.X", " XPICK.X")).contains(&repeated("DOMAIN")));
    }
}

/// A 1.0 specification whose two row templates, built from `DM`, derive the
/// same three columns, none of which has a column-level derivation.
const REPEATED_1_0: &str = r#"yamaa: "1.0"
type: spec
domain: DM
keys: [STUDYID, USUBJID]
input:
  DM: input/dm.csv
  SV: input/sv.csv

output:
  path: dm.csv
  columns: [STUDYID, USUBJID, DOMAIN]

columns:
  - name: STUDYID
    type: str
    label: Study Identifier

  - name: USUBJID
    type: str
    label: Unique Subject Identifier

  - name: DOMAIN
    type: str
    label: Domain Abbreviation

rows:
  - id: a
    dataset: DM
    derivations:
      STUDYID: DM.STUDYID
      USUBJID: DM.USUBJID
      DOMAIN: "'DM'"

  - id: b
    dataset: DM
    derivations:
      STUDYID: DM.STUDYID
      USUBJID: DM.USUBJID
      DOMAIN: "'DM'"
"#;

/// The 1.0 finding for `column`, at the first template's derivation, which
/// the 1.x spec path notation names by the template's `id`.
fn repeated_1_0(column: &str) -> (&'static str, String) {
    (
        "repeated_row_derivation",
        format!("rows.a.derivations.{column}"),
    )
}

/// REPEATED_1_0 with `derivation` as both templates' `DOMAIN`, and `extra`
/// written before `output`.
fn repeated_domain(derivation: &str, extra: &str) -> String {
    REPEATED_1_0
        .replace(
            "      DOMAIN: \"'DM'\"\n",
            &format!("      DOMAIN: {derivation}\n"),
        )
        .replace("\noutput:\n", &format!("{extra}\noutput:\n"))
}

#[test]
fn a_1_0_repeated_row_derivation_follows_spec_6() {
    assert_eq!(
        paths(REPEATED_1_0),
        [
            repeated_1_0("STUDYID"),
            repeated_1_0("USUBJID"),
            repeated_1_0("DOMAIN")
        ]
    );
    // A read of an intermediate an input backs is row-local, so it moves to
    // column level; a read of a `SELF` intermediate is not.
    let backed = "intermediates:\n  - id: PICK\n    dataset: SV\n    key: [STUDYID, USUBJID]\n    \
                  filter: \"SV.VISITNUM = 1\"\n";
    assert!(paths(&repeated_domain("PICK.SVSTDTC", backed)).contains(&repeated_1_0("DOMAIN")));
    let earlier = "intermediates:\n  - id: PRIOR\n    dataset: SELF\n    key: [STUDYID, USUBJID]\n";
    assert!(!paths(&repeated_domain("PRIOR.DOMAIN", earlier)).contains(&repeated_1_0("DOMAIN")));
    // An aggregate or a window anywhere in the derivation, a `case` result
    // included, makes it not row-local.
    for derivation in [
        "{aggregate: \"MAX(SV.VISITNUM)\"}",
        "{case: [{when: \"DM.AGE > 1\", then: {aggregate: \"MAX(SV.VISITNUM)\"}}, \
         {otherwise: null}]}",
        "{case: [{when: \"DM.AGE > 1\", then: {row_number: {}}}, {otherwise: null}]}",
    ] {
        let text = repeated_domain(derivation, "");
        assert!(!paths(&text).contains(&repeated_1_0("DOMAIN")), "{text}");
        assert!(paths(&text).contains(&repeated_1_0("STUDYID")), "{text}");
    }
    // So does a read of a column whose column-level derivation is not
    // row-local, also through a structured source's `between` value (D118).
    let blocked = |derivation: &str| {
        repeated_domain(derivation, "").replace(
            "  - name: DOMAIN\n",
            "  - name: VISITS\n    type: int\n    derivation: {aggregate: \"COUNT(SV.*)\"}\n\n  \
             - name: DOMAIN\n",
        )
    };
    for derivation in [
        "VISITS",
        "{source: {variable: SV.SVSTDTC, between: {value: VISITS, lower: LO, upper: HI}}}",
    ] {
        let text = blocked(derivation);
        assert!(!paths(&text).contains(&repeated_1_0("DOMAIN")), "{text}");
    }
    assert!(paths(&blocked("SV.SVSTDTC")).contains(&repeated_1_0("DOMAIN")));
}

#[test]
fn a_1_0_literal_is_quoted_text_a_number_or_null() {
    // Templates built from different inputs bind different records, so only
    // a driver-independent literal moves to column level.
    let mixed = |derivation: &str| {
        repeated_domain(derivation, "").replace(
            "  - id: b\n    dataset: DM\n",
            "  - id: b\n    dataset: SV\n",
        )
    };
    for (derivation, reported) in [
        ("\"'DM'\"", true),
        ("1", true),
        ("1.5", true),
        ("null", true),
        ("DM.DOMAIN", false),
        ("\"DM.DOMAIN || 'X'\"", false),
        // Grammar text for a number or the missing value is the lint's other
        // finding; it is no quoted text.
        ("\"NULL\"", false),
        ("true", false),
    ] {
        let text = mixed(derivation);
        let found = paths(&text);
        assert!(!found.contains(&repeated_1_0("STUDYID")), "{text}");
        assert_eq!(found.contains(&repeated_1_0("DOMAIN")), reported, "{text}");
    }
}

/// A 1.0 specification with a named intermediate that pairs its input's
/// declared key columns with the same-named values.
const REDUNDANT_1_0: &str = r#"yamaa: "1.0"
type: spec
domain: ADAE
keys: [STUDYID, USUBJID, AESEQ]
input:
  AE: input/ae.csv
  ADSL: {path: input/adsl.csv, key: [STUDYID, USUBJID]}
base: AE

output:
  path: adae.csv
  columns: [STUDYID, USUBJID, AESEQ, AGE, SEX]

intermediates:
  - id: ADSL1
    dataset: ADSL
    key: {STUDYID: STUDYID, USUBJID: USUBJID}

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: AE.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: AE.USUBJID

  - name: AESEQ
    type: int
    label: Sequence Number
    derivation: AE.AESEQ

  - name: AGE
    type: int
    label: Age
    derivation: ADSL1.AGE

  - name: SEX
    type: str
    label: Sex
    derivation: ADSL.SEX
"#;

#[test]
fn a_1_0_redundant_intermediate_restates_its_input_s_key() {
    let found = findings(REDUNDANT_1_0);
    assert_eq!(
        found
            .iter()
            .map(|item| (
                item.finding,
                item.spec_path.as_str(),
                item.line,
                item.column
            ))
            .collect::<Vec<_>>(),
        [("redundant_intermediate", "intermediates.ADSL1", 15, 5)]
    );
    assert_eq!(
        found[0].message,
        "intermediate `ADSL1` reads exactly what the implicit join to `ADSL` reads; read `ADSL` directly"
    );
    let key = "    key: {STUDYID: STUDYID, USUBJID: USUBJID}\n";
    let with = |line: &str| REDUNDANT_1_0.replace(key, &format!("{key}{line}"));
    let redundant = |text: &str| names(text).contains(&"redundant_intermediate");
    // `absent` omitted or null answers with missing, as the join does; any
    // other answer, a requirement or a narrowing step changes what is read.
    assert!(redundant(&with("    absent: null\n")));
    for line in [
        "    absent: 0\n",
        "    require_match: true\n",
        "    filter: \"ADSL.SAFFL = 'Y'\"\n",
        "    order_by: [ADSL.TRTSDT]\n    keep: first\n",
    ] {
        assert!(!redundant(&with(line)), "{line}");
    }
    // The 1.0 meaning compares the key with the input's declared key, not
    // with the output keys (D111, LOOKUP-5).
    let other_pairs = REDUNDANT_1_0.replace(key, "    key: {STUDYID: STUDYID, USUBJID: SUBJID}\n");
    assert!(!redundant(&other_pairs));
    let narrower_input = REDUNDANT_1_0.replace(
        "  ADSL: {path: input/adsl.csv, key: [STUDYID, USUBJID]}",
        "  ADSL: {path: input/adsl.csv, key: [USUBJID]}",
    );
    assert!(!redundant(&narrower_input));
    let narrower_key = REDUNDANT_1_0
        .replace(key, "    key: {USUBJID: USUBJID}\n")
        .replace(
            "  ADSL: {path: input/adsl.csv, key: [STUDYID, USUBJID]}",
            "  ADSL: {path: input/adsl.csv, key: [USUBJID]}",
        );
    assert!(redundant(&narrower_key));
    // An input without `key` has no implicit join, and one that requires a
    // match fails where the intermediate answers.
    for input in [
        "  ADSL: input/adsl.csv",
        "  ADSL: {path: input/adsl.csv, key: [STUDYID, USUBJID], require_match: true}",
    ] {
        let text = REDUNDANT_1_0.replace(
            "  ADSL: {path: input/adsl.csv, key: [STUDYID, USUBJID]}",
            input,
        );
        assert!(!redundant(&text), "{input}");
    }
    // The key written as the input's list fails validation as
    // `rename_only_intermediate` (LOOKUP-4), which the lint leaves to it.
    let listed = REDUNDANT_1_0.replace(key, "    key: [USUBJID, STUDYID]\n");
    assert!(!redundant(&listed));
    // An intermediate no column reads is unused, not redundant.
    let unread = REDUNDANT_1_0.replace("derivation: ADSL1.AGE", "derivation: ADSL.AGE");
    assert!(!redundant(&unread));
}

#[test]
fn a_1_0_file_names_members_in_the_spec_path_notation() {
    // A list entry that writes `name`, or else `id`, as text is named by it,
    // as the engine's findings name it (P-CLR-2); any other by its index.
    let packed = REDUNDANT_1_0.replace(
        "AE.STUDYID\n\n  - name: USUBJID",
        "AE.STUDYID\n  - name: USUBJID",
    );
    assert_eq!(
        paths(&packed),
        [
            ("redundant_intermediate", "intermediates.ADSL1".to_owned()),
            ("entry_spacing", "columns.USUBJID".to_owned()),
        ]
    );
    let unnamed = packed.replace("  - name: USUBJID\n", "  - name: 1\n");
    assert!(paths(&unnamed).contains(&("entry_spacing", "columns[1]".to_owned())));
    let unlabelled = REDUNDANT_1_0.replace("    label: Age\n", "");
    assert_eq!(
        paths(&unlabelled),
        [
            ("redundant_intermediate", "intermediates.ADSL1".to_owned()),
            ("missing_label", "columns.AGE".to_owned()),
        ]
    );
    // A key that is no bare segment is quoted, and a list without identities
    // takes indexes.
    let long = format!(
        "{REDUNDANT_1_0}\nmetadata:\n  study note:\n    \"{}\"\n",
        "x".repeat(76)
    );
    assert!(paths(&long).contains(&("line_width", "metadata['study note']".to_owned())));
    let order = REDUNDANT_1_0.replace(
        "  columns: [STUDYID, USUBJID, AESEQ, AGE, SEX]\n",
        "  order_by:\n    - {variable: AESEQ, direction: desc}\n  columns: [STUDYID, USUBJID, AESEQ, AGE, SEX]\n",
    );
    assert!(paths(&order).contains(&("field_order", "output.columns".to_owned())));
}

/// A standalone 1.0 specification written in its canonical spellings.
const SPELLED: &str = r#"yamaa: "1.0"
type: spec
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  DM: input/dm.csv
  EX: input/ex.csv
  ADSL: {path: input/adsl.csv, key: [STUDYID, USUBJID]}
base: DM

output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, AGE, TRTSDT]

intermediates:
  - id: FIRSTDOSE
    dataset: EX
    key: [STUDYID, USUBJID]
    order_by: [EX.EXSTDTC]
    keep: first

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: DM.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: DM.USUBJID

  - name: AGE
    type: int
    label: Age
    derivation: DM.AGE

  - name: TRTSDT
    type: date
    label: Date of First Exposure to Treatment
    derivation: ADSL.TRTSDT
"#;

/// A finding as (finding, spec path, message).
type Expected<'a> = (&'a str, &'a str, &'a str);

/// Each spelling written into SPELLED in place of `canonical`: the findings
/// it gives, and that the fix writes SPELLED back.
fn spellings(cases: &[(&str, &str, &[Expected])]) {
    assert!(findings(SPELLED).is_empty());
    for &(canonical, written, expected) in cases {
        assert!(SPELLED.contains(canonical), "{canonical}");
        let text = SPELLED.replace(canonical, written);
        assert_eq!(
            findings(&text)
                .iter()
                .map(|item| (item.finding, item.spec_path.as_str(), item.message.as_str()))
                .collect::<Vec<_>>(),
            expected,
            "{text}"
        );
        let after = fixed(&text);
        assert_eq!(after, SPELLED, "{text}");
        // The fix is settled: fixing the fixed file changes nothing.
        assert_eq!(fixed(&after), after);
    }
}

#[test]
fn restated_defaults_are_removed() {
    spellings(&[
        (
            "  ADSL: {path: input/adsl.csv, key: [STUDYID, USUBJID]}\n",
            "  ADSL: {path: input/adsl.csv, relative_to: declaring, key: [STUDYID, USUBJID]}\n",
            &[(
                "restated_default",
                "input.ADSL.relative_to",
                "`relative_to: declaring` restates the default; remove it",
            )],
        ),
        (
            "  ADSL: {path: input/adsl.csv, key: [STUDYID, USUBJID]}\n",
            "  ADSL: {path: input/adsl.csv, empty_string: missing, key: [STUDYID, USUBJID]}\n",
            &[(
                "restated_default",
                "input.ADSL.empty_string",
                "`empty_string: missing` restates the default; remove it",
            )],
        ),
        // The first entry of a flow mapping goes with the separator after it.
        (
            "  ADSL: {path: input/adsl.csv, key: [STUDYID, USUBJID]}\n",
            "  ADSL: {empty_string: missing, path: input/adsl.csv, key: [STUDYID, USUBJID]}\n",
            &[
                (
                    "restated_default",
                    "input.ADSL.empty_string",
                    "`empty_string: missing` restates the default; remove it",
                ),
                (
                    "field_order",
                    "input.ADSL.path",
                    "write `path` before `empty_string`",
                ),
            ],
        ),
        // A handler whose omission answers with missing restates it as null.
        (
            "    keep: first\n",
            "    keep: first\n    absent: null\n",
            &[(
                "restated_default",
                "intermediates.FIRSTDOSE.absent",
                "`absent: null` restates the default; remove it",
            )],
        ),
    ]);
}

#[test]
fn a_removed_default_that_leaves_a_long_form_is_written_as_the_shorthand() {
    // Removing a source binding's own absence answer leaves the binding
    // holding only its variable, which the next pass writes as the bare
    // read (EXPR-5); a written answer keeps the binding.
    let source = |fields: &str| {
        SPELLED.replace(
            "    derivation: DM.AGE\n",
            &format!("    derivation:\n      source: {{variable: DM.AGE{fields}}}\n"),
        )
    };
    assert_eq!(
        paths(&source(", absent: null")),
        [(
            "restated_default",
            "columns.AGE.derivation.source.absent".to_owned()
        )]
    );
    assert_eq!(fixed(&source(", absent: null")), SPELLED);
    assert!(names(&source(", absent: 0")).is_empty());
    // Removing `direction: asc` leaves `{variable: X}`, which the next pass
    // writes as its shorthand.
    let text = SPELLED.replace(
        "    order_by: [EX.EXSTDTC]\n",
        "    order_by:\n      - {variable: EX.EXSTDTC, direction: asc}\n",
    );
    assert_eq!(
        paths(&text),
        [(
            "restated_default",
            "intermediates.FIRSTDOSE.order_by[0].direction".to_owned()
        )]
    );
    assert_eq!(
        fixed(&text),
        SPELLED.replace(
            "    order_by: [EX.EXSTDTC]\n",
            "    order_by:\n      - EX.EXSTDTC\n",
        )
    );
    // A chain the fixes cannot finish leaves the file as written: here the
    // comment above `path` keeps the input's long form from its key line.
    let stuck = SPELLED.replace(
        "  EX: input/ex.csv\n",
        "  EX:\n    # The exposure records.\n    path: input/ex.csv\n    relative_to: declaring\n",
    );
    assert_eq!(names(&stuck), ["restated_default"]);
    assert_eq!(fixed(&stuck), stuck);
}

#[test]
fn a_restated_default_needs_its_meaning_proved() {
    // A handler whose omission fails declares an answer when it is null.
    let unmapped = SPELLED.replace(
        "    derivation: DM.AGE\n",
        "    derivation:\n      mapping: {source: DM.AGEU, dict: {Y: 1}, unmapped: null}\n",
    );
    assert!(!names(&unmapped).contains(&"restated_default"));
    // A flag's `missing` beside a declared `false_value` is required (EXPR-8).
    let flag = |fields: &str| {
        SPELLED.replace(
            "    derivation: DM.AGE\n",
            &format!("    derivation:\n      flag: {{condition: \"DM.AGE > 1\", {fields}}}\n"),
        )
    };
    assert!(!names(&flag("false_value: N, missing: null")).contains(&"restated_default"));
    assert_eq!(
        paths(&flag("false_value: null, missing: U")),
        [(
            "restated_default",
            "columns.AGE.derivation.flag.false_value".to_owned()
        )]
    );
    // An absence answer beside `require_match: true` conflicts with it,
    // which validation refuses (LOOKUP-24).
    let conflicting = SPELLED.replace(
        "    keep: first\n",
        "    keep: first\n    absent: null\n    require_match: true\n",
    );
    assert!(!names(&conflicting).contains(&"restated_default"));
    // A layer may replace an inherited value, so it is not read, whether it
    // names a parent or includes a fragment (LAYER-3).
    for header in ["parent: base.yaml\n", "include: [fragment.yaml]\n"] {
        let layer = SPELLED.replace("domain: ADSL\n", &format!("{header}domain: ADSL\n"));
        let restated = layer.replace("    keep: first\n", "    keep: first\n    absent: null\n");
        assert!(!names(&restated).contains(&"restated_default"), "{header}");
    }
    // A comment the removal would lose keeps the entry as written.
    let commented = SPELLED.replace(
        "    keep: first\n",
        "    keep: first\n    absent: null  # as the join answers\n",
    );
    assert_eq!(names(&commented), ["restated_default"]);
    assert_eq!(fixed(&commented), commented);
    // A suppression covers the entry it stands above.
    let suppressed = SPELLED.replace(
        "  - id: FIRSTDOSE\n",
        "  # yamaa-lint: allow restated_default -- the answer is stated for review\n  \
         - id: FIRSTDOSE\n",
    );
    let suppressed = suppressed.replace("    keep: first\n", "    keep: first\n    absent: null\n");
    assert!(names(&suppressed).is_empty());
    assert_eq!(fixed(&suppressed), suppressed);
}

#[test]
fn restated_defaults_in_project_codelists_and_function_files() {
    let project = PROJECT.replace(
        "  define: sdtm/define.xml\n",
        "  define: sdtm/define.xml\n  context: Submission\n  xml_lang: en\n",
    );
    assert_eq!(
        paths(&project),
        [
            ("restated_default", "sdtm.context".to_owned()),
            ("restated_default", "sdtm.xml_lang".to_owned()),
        ]
    );
    assert_eq!(fixed(&project), PROJECT);
    let codelists = CODELISTS.replace(
        "    name: Age Unit\n",
        "    name: Age Unit\n    data_type: text\n    extensible: false\n",
    );
    assert_eq!(
        paths(&codelists),
        [
            (
                "restated_default",
                "codelists['Age Unit'].data_type".to_owned()
            ),
            (
                "restated_default",
                "codelists['Age Unit'].extensible".to_owned()
            ),
        ]
    );
    assert_eq!(fixed(&codelists), CODELISTS);
    let function = FUNCTION
        .replace(
            "returns: float\n",
            "returns: float\nmay_return_missing: false\n",
        )
        .replace(
            "  - {name: height_cm, type: float}\n",
            "  - {name: height_cm, type: float, required: true}\n",
        );
    assert_eq!(
        paths(&function),
        [
            ("restated_default", "params.height_cm.required".to_owned()),
            ("restated_default", "may_return_missing".to_owned()),
        ]
    );
    assert_eq!(fixed(&function), FUNCTION);
}

#[test]
fn grammar_text_for_a_scalar_is_written_as_the_scalar() {
    let at = |derivation: &str| format!("    derivation: {derivation}\n");
    for (written, scalar, what) in [
        ("\"0\"", "0", "a number"),
        ("\"-1.5\"", "-1.5", "a number"),
        ("\"NULL\"", "null", "the missing value"),
        ("\"(2)\"", "2", "a number"),
    ] {
        let text = SPELLED.replace(&at("DM.AGE"), &at(written));
        assert_eq!(
            findings(&text)
                .iter()
                .map(|item| (item.finding, item.spec_path.as_str(), item.message.as_str()))
                .collect::<Vec<_>>(),
            [(
                "scalar_as_text",
                "columns.AGE.derivation",
                format!("`{written}` is grammar text for {what}; write the YAML scalar `{scalar}`")
                    .as_str()
            )],
            "{text}"
        );
        assert_eq!(fixed(&text), SPELLED.replace(&at("DM.AGE"), &at(scalar)));
    }
    // A case result, a function argument and a key match value are
    // expression fields too.
    for (canonical, written, path) in [
        (
            "    derivation: DM.AGE\n",
            "    derivation:\n      case:\n        - when: \"DM.AGE > 1\"\n          then: \"1\"\n        \
             - otherwise: DM.AGE\n",
            "columns.AGE.derivation.case[0].then",
        ),
        (
            "    derivation: DM.AGE\n",
            "    derivation:\n      function: {name: scale, args: {value: DM.AGE, by: \"2\"}}\n",
            "columns.AGE.derivation.function.args.by",
        ),
        (
            "    key: [STUDYID, USUBJID]\n    order_by",
            "    key: {STUDYID: STUDYID, USUBJID: USUBJID, EXSEQ: \"1\"}\n    order_by",
            "intermediates.FIRSTDOSE.key.EXSEQ",
        ),
    ] {
        let text = SPELLED.replace(canonical, written);
        assert_eq!(paths(&text), [("scalar_as_text", path.to_owned())], "{text}");
    }
    // Quoted text, a name, a predicate, a template and a value field keep
    // their text.
    for (canonical, written) in [
        ("    derivation: DM.AGE\n", "    derivation: \"'0'\"\n"),
        ("    derivation: DM.AGE\n", "    derivation: \"AGE\"\n"),
        (
            "    derivation: DM.AGE\n",
            "    derivation: {str_template: \"0\"}\n",
        ),
        (
            "    derivation: DM.AGE\n",
            "    derivation: {flag: {condition: \"TRUE\", true_value: \"1\"}}\n",
        ),
        (
            "    keep: first\n",
            "    keep: first\n    filter: \"TRUE\"\n",
        ),
    ] {
        let text = SPELLED.replace(canonical, written);
        assert!(!names(&text).contains(&"scalar_as_text"), "{text}");
    }
}

#[test]
fn a_long_form_is_written_as_its_shorthand() {
    spellings(&[
        (
            "  EX: input/ex.csv\n",
            "  EX: {path: input/ex.csv}\n",
            &[(
                "redundant_long_form",
                "input.EX",
                "the long form of `path` only restates its shorthand; write `input/ex.csv`",
            )],
        ),
        (
            "  EX: input/ex.csv\n",
            "  EX:\n    path: input/ex.csv\n",
            &[(
                "redundant_long_form",
                "input.EX",
                "the long form of `path` only restates its shorthand; write `input/ex.csv`",
            )],
        ),
        (
            "    order_by: [EX.EXSTDTC]\n",
            "    order_by: [{variable: EX.EXSTDTC}]\n",
            &[(
                "redundant_long_form",
                "intermediates.FIRSTDOSE.order_by[0]",
                "the long form of `variable` only restates its shorthand; write `EX.EXSTDTC`",
            )],
        ),
        (
            "    derivation: DM.AGE\n",
            "    derivation: {value: DM.AGE}\n",
            &[(
                "redundant_long_form",
                "columns.AGE.derivation",
                "the long form of `value` only restates its shorthand; write `DM.AGE`",
            )],
        ),
        // A structured source that holds only its variable is replaced, as
        // the operation it is, by the bare read (EXPR-5).
        (
            "    derivation: DM.AGE\n",
            "    derivation: {source: {variable: DM.AGE}}\n",
            &[(
                "redundant_long_form",
                "columns.AGE.derivation.source",
                "the long form of `variable` only restates its shorthand; write `DM.AGE`",
            )],
        ),
        (
            "    derivation: DM.AGE\n",
            "    derivation:\n      source: {variable: DM.AGE}\n",
            &[(
                "redundant_long_form",
                "columns.AGE.derivation.source",
                "the long form of `variable` only restates its shorthand; write `DM.AGE`",
            )],
        ),
    ]);
    // A bare keyword written as an empty entry, and a block payload.
    let verified = |check: &str| {
        SPELLED.replace(
            "    derivation: DM.AGE\n",
            &format!("    derivation: DM.AGE\n    verifications:\n      - {check}\n"),
        )
    };
    let text = verified("not_missing: {}");
    assert_eq!(
        findings(&text)
            .iter()
            .map(|item| (item.finding, item.spec_path.as_str(), item.message.as_str()))
            .collect::<Vec<_>>(),
        [(
            "redundant_long_form",
            "columns.AGE.verifications[0]",
            "the long form of `not_missing` only restates its shorthand; write `not_missing`"
        )]
    );
    assert_eq!(fixed(&text), verified("not_missing"));
    let aggregate = |payload: &str| {
        SPELLED.replace(
            "    derivation: DM.AGE\n",
            &format!("    derivation:\n      aggregate:{payload}\n"),
        )
    };
    let text = aggregate("\n        expr: \"MAX(EX.EXDOSE)\"");
    assert_eq!(names(&text), ["redundant_long_form"]);
    assert_eq!(fixed(&text), aggregate(" \"MAX(EX.EXDOSE)\""));
    // A shorthand written over several lines has no place this fix writes,
    // so the finding stays.
    let text = format!("{SPELLED}\nverifications:\n  - unique:\n      columns:\n        - AGE\n");
    assert_eq!(
        paths(&text),
        [("redundant_long_form", "verifications[0].unique".to_owned())]
    );
    assert_eq!(fixed(&text), text);
}

/// A 1.0 specification built from `LB` whose two other inputs declare keys
/// that a read uses: `ADSL` through an implicit join, `RANGES` through a
/// keyless qualified aggregate.
const KEYED: &str = r#"yamaa: "1.0"
type: spec
domain: ADLB
keys: [STUDYID, USUBJID, LBSEQ]
input:
  LB: input/lb.csv
  ADSL: {path: input/adsl.csv, key: [STUDYID, USUBJID]}
  RANGES: {path: input/ranges.csv, key: [STUDYID, USUBJID]}
base: LB

output:
  path: adlb.csv
  columns: [STUDYID, USUBJID, LBSEQ, AGE, RANGES]

intermediates:
  - id: RANGE
    dataset: RANGES
    key: [LBTESTCD]
    filter: "RANGES.SEX = 'F'"

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: LB.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: LB.USUBJID

  - name: LBSEQ
    type: int
    label: Sequence Number
    derivation: LB.LBSEQ

  - name: AGE
    type: int
    label: Age
    derivation: ADSL.AGE

  - name: RANGES
    type: int
    label: Ranges per Subject
    derivation: {aggregate: "COUNT(RANGES.*)"}
"#;

#[test]
fn an_input_key_no_read_uses_is_reported() {
    assert!(findings(KEYED).is_empty());
    let message = |dataset: &str| {
        format!(
            "no implicit join or keyless aggregate reads input `{dataset}` by its declared \
             `key`; remove the `key`"
        )
    };
    // The base input's reads read the driver, so its key is unused.
    let base = KEYED.replace(
        "  LB: input/lb.csv\n",
        "  LB: {path: input/lb.csv, key: [STUDYID, USUBJID, LBSEQ]}\n",
    );
    let found = findings(&base);
    assert_eq!(
        found
            .iter()
            .map(|item| (item.finding, item.spec_path.as_str(), item.message.clone()))
            .collect::<Vec<_>>(),
        [("unused_input_key", "input.LB.key", message("LB"))]
    );
    assert_eq!((found[0].line, found[0].column), (6, 28));
    // No fix is offered: removing a key is the author's call.
    assert_eq!(fixed(&base), base);
    // An aggregate that declares its own key, and a named intermediate over
    // the input, read it by their own keys.
    let own_key = KEYED.replace(
        "{aggregate: \"COUNT(RANGES.*)\"}",
        "{aggregate: {key: [USUBJID], expr: \"COUNT(RANGES.*)\"}}",
    );
    assert_eq!(
        paths(&own_key),
        [("unused_input_key", "input.RANGES.key".to_owned())]
    );
    // A read through the implicit join uses the key, and so does a copy.
    let unread = KEYED.replace("derivation: ADSL.AGE", "derivation: LB.AGE");
    assert_eq!(
        paths(&unread),
        [("unused_input_key", "input.ADSL.key".to_owned())]
    );
    let copied = unread.replace(
        "  - name: AGE\n    type: int\n    label: Age\n    derivation: LB.AGE\n",
        "  - copy: {dataset: ADSL, columns: [AGE]}\n",
    );
    assert!(!names(&copied).contains(&"unused_input_key"), "{copied}");
    // A template built from another input reads the base input through the
    // join (NAME-4).
    let templates = base.replace("base: LB\n", "").replace(
        "    derivation: {aggregate: \"COUNT(RANGES.*)\"}\n",
        "    derivation: {aggregate: \"COUNT(RANGES.*)\"}\n\nrows:\n  - id: lab\n    \
             dataset: LB\n\n  - id: subject\n    dataset: ADSL\n    derivations:\n      \
             LBSEQ: 0\n",
    );
    assert!(
        !names(&templates).contains(&"unused_input_key"),
        "{templates}"
    );
    // A layer's columns may come from the layer it inherits or a fragment it
    // includes.
    for header in ["parent: base.yaml\n", "include: [fragment.yaml]\n"] {
        let layer = base.replace("domain: ADLB\n", &format!("{header}domain: ADLB\n"));
        assert!(!names(&layer).contains(&"unused_input_key"), "{header}");
    }
    // A shared parent layer suppresses it with a reason.
    let shared = base.replace(
        "  LB: {path",
        "  # yamaa-lint: allow unused_input_key -- child layers join LB\n  LB: {path",
    );
    assert!(names(&shared).is_empty());
}

#[test]
fn an_equality_with_a_current_row_value_in_a_lookup_filter_is_a_key_pair() {
    let filtered = |filter: &str| KEYED.replace("RANGES.SEX = 'F'", filter);
    let message = |conjunct: &str| {
        format!(
            "`{conjunct}` compares a donor field with a current-row value; declare the pair \
             in `key`"
        )
    };
    let found = findings(&filtered(
        "RANGES.SEX = 'F' AND RANGES.LBTESTCD = LB.LBTESTCD",
    ));
    assert_eq!(
        found
            .iter()
            .map(|item| (item.finding, item.spec_path.as_str(), item.message.clone()))
            .collect::<Vec<_>>(),
        [(
            "equality_filter",
            "intermediates.RANGE.filter",
            message("RANGES.LBTESTCD = LB.LBTESTCD")
        )]
    );
    assert_eq!((found[0].line, found[0].column), (19, 13));
    assert_eq!(
        fixed(&filtered("LB.LBTESTCD = RANGES.LBTESTCD")),
        filtered("LB.LBTESTCD = RANGES.LBTESTCD")
    );
    // Either side, inside grouping, and an expression of the current row.
    for filter in [
        "(LB.LBTESTCD = RANGES.LBTESTCD)",
        "RANGES.SEX = 'F' AND (RANGES.AGE = LB.AGE + 1)",
    ] {
        assert_eq!(names(&filtered(filter)), ["equality_filter"], "{filter}");
    }
    // A constant, another comparison, a disjunction, a negation and a
    // comparison of two donor fields stay in `filter`.
    for filter in [
        "RANGES.SEX = 'F'",
        "RANGES.LOW < LB.AGE",
        "RANGES.SEX = LB.SEX OR RANGES.SEX = 'U'",
        "NOT (RANGES.SEX = LB.SEX)",
        "RANGES.LOW = RANGES.HIGH",
    ] {
        assert!(names(&filtered(filter)).is_empty(), "{filter}");
    }
    // A source binding's filter narrows its variable's input, and a
    // qualified aggregate's the one input its expression reads.
    let source = KEYED.replace(
        "    derivation: ADSL.AGE\n",
        "    derivation:\n      source: {variable: ADSL.AGE, filter: \"ADSL.SITEID = LB.SITEID\"}\n",
    );
    assert_eq!(
        paths(&source),
        [(
            "equality_filter",
            "columns.AGE.derivation.source.filter".to_owned()
        )]
    );
    let aggregate = KEYED.replace(
        "    derivation: {aggregate: \"COUNT(RANGES.*)\"}\n",
        "    derivation:\n      aggregate:\n        filter: \"RANGES.VISIT = LB.VISIT\"\n        \
         expr: \"COUNT(RANGES.*)\"\n",
    );
    assert_eq!(
        paths(&aggregate),
        [(
            "equality_filter",
            "columns.RANGES.derivation.aggregate.filter".to_owned()
        )]
    );
}

const MISSING_LABEL: &str = r#"yamaa: "1.0"
type: spec
domain: ADSL
keys: [STUDYID]
input:
  DM: input/dm.csv

output:
  path: adsl.csv
  columns: [STUDYID, AGE]

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: DM.STUDYID

  - name: AGE
    type: int
    derivation: DM.AGE
"#;

#[test]
fn output_columns_without_a_label_are_reported() {
    let found = findings(MISSING_LABEL);
    assert_eq!(
        found
            .iter()
            .map(|item| (
                item.finding,
                item.spec_path.as_str(),
                item.line,
                item.column
            ))
            .collect::<Vec<_>>(),
        [("missing_label", "columns.AGE", 18, 5)]
    );
    assert_eq!(
        found[0].message,
        "output column `AGE` has no label; declare `label:`"
    );
    assert_eq!(fixed(MISSING_LABEL), MISSING_LABEL);
    // A stray entry leaves the column named by its name, and the finding is
    // at the column's own name.
    let stray = MISSING_LABEL.replace(
        "columns:\n  - name: STUDYID\n",
        "columns:\n  - stray\n\n  - name: STUDYID\n",
    );
    let found = findings(&stray);
    assert_eq!(
        found
            .iter()
            .map(|item| (item.spec_path.as_str(), item.line))
            .collect::<Vec<_>>(),
        [("columns.AGE", 20)]
    );
    // A column the output lists twice has one label departure.
    let repeated = MISSING_LABEL.replace(
        "  columns: [STUDYID, AGE]",
        "  columns: [STUDYID, AGE, AGE]",
    );
    assert_eq!(
        paths(&repeated),
        [("missing_label", "columns.AGE".to_owned())]
    );
}

#[test]
fn internal_and_inherited_columns_need_no_label() {
    let internal = MISSING_LABEL.replace("  columns: [STUDYID, AGE]", "  columns: [STUDYID]");
    assert!(names(&internal).is_empty());
    for header in ["parent: base.yaml", "include: [fragment.yaml]"] {
        let inherited = MISSING_LABEL.replace(
            "type: spec\ndomain: ADSL",
            &format!("type: spec\n{header}\ndomain: ADSL"),
        );
        assert!(!names(&inherited).contains(&"missing_label"), "{header}");
    }
}

#[test]
fn an_authoring_suppression_with_a_reason_covers_its_entry() {
    let text = REPEATED.replace(
        "  - id: a",
        "  # yamaa-lint: allow repeated_row_derivation -- reviewed row scope\n  - id: a",
    );
    assert!(names(&text).is_empty());
}

#[test]
fn an_empty_document_has_no_findings_but_its_suppressions_are_read() {
    assert!(findings("").is_empty());
    assert!(findings("# a comment\n").is_empty());
    assert_eq!(
        names("# yamaa-lint: allow nothing\n"),
        ["invalid_suppression"]
    );
}

#[test]
fn lint_faults_keep_their_existing_codes_and_resource_facts() {
    use super::Error;
    use yamaa_core::{diagnostic::ConditionCode, json::Value};

    for (error, context) in [
        (
            Error::Unavailable,
            crate::json!({"step":"prepare", "code":"yaml_reader_unavailable"}),
        ),
        (
            Error::Source {
                file: "spec.yaml".into(),
                failure: crate::yaml_decode::DecodeFailure::Internal,
            },
            crate::json!({"step":"prepare", "code":"yaml_admission"}),
        ),
        (
            Error::Write {
                file: "spec.yaml".into(),
                error: crate::file_publication::Error::Io(std::io::Error::other("write failed")),
            },
            crate::json!({"step":"output", "code":"output_publication_failed"}),
        ),
        (
            Error::Write {
                file: "spec.yaml".into(),
                error: crate::file_publication::Error::Host(
                    crate::host_resources::Failure::Cancelled,
                ),
            },
            crate::json!({"step":"output", "code":"host_publication_failure"}),
        ),
        (
            Error::Write {
                file: "spec.yaml".into(),
                error: crate::file_publication::Error::Host(
                    crate::host_resources::Failure::Cleanup,
                ),
            },
            crate::json!({"step":"output", "code":"host_publication_failure"}),
        ),
        (
            Error::Write {
                file: "spec.yaml".into(),
                error: crate::file_publication::Error::Limit(yamaa_core::limits::LimitExceeded {
                    resource: "output_bytes",
                    limit: 1024,
                    actual: Some(1025),
                }),
            },
            crate::json!({"step":"output", "code":"publication_limit",
                "resource":"output_bytes", "limit":1024, "actual":1025}),
        ),
    ] {
        let diagnostics = error.diagnostics();
        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.code, ConditionCode::ApplicationRejected);
        assert_eq!(diagnostic.rule(), None);
        assert!(diagnostic.spec_paths.is_empty());
        assert_eq!(Value::Object(diagnostic.context.clone()), context);
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
    }
}

mod files {
    use super::{super::lint, CANONICAL, VERSIONED};
    use std::path::{Path, PathBuf};
    use yamaa_core::diagnostic::ConditionCode;

    struct Directory(PathBuf);

    impl Directory {
        fn new(name: &str) -> Self {
            Self::under(&std::env::temp_dir(), name)
        }

        fn under(root: &Path, name: &str) -> Self {
            let path = root.join(format!(
                "yamaa-lint-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, name: &str, text: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            path
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn text(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    fn display(path: &Path) -> String {
        std::path::absolute(path)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
    }

    #[test]
    fn directories_expand_to_specifications_in_sorted_order() {
        let directory = Directory::new("expand");
        let broken = CANONICAL.replace("base: DM\n\noutput:", "base: DM\noutput:");
        let second = directory.write("b/adae.yaml", &broken);
        let first = directory.write("a-b.yaml", &broken);
        let third = directory.write("b/c/adsl.yaml", &broken);
        directory.write(
            "environment.yaml",
            "schema_version: \"1.0\"\nstudy: {id: S1}\n",
        );
        directory.write("yamaa-project.yaml", "data_roots: [data]\n");
        directory.write(".cache/copy.yaml", &broken);
        directory.write("sequence-key.yaml", "? [a, b]\n: 1\nschema_version: 1\n");
        directory.write("mapping-key.yaml", "? {a: 1}\n: 2\n");
        directory.write("notes.txt", &broken);
        let linted = lint(&[text(&directory.0)], false).unwrap();
        assert_eq!(
            linted
                .iter()
                .map(|file| file.file.clone())
                .collect::<Vec<_>>(),
            [display(&first), display(&second), display(&third)]
        );
        assert!(linted
            .iter()
            .all(|file| file.findings.len() == 1 && file.findings[0].finding == "root_spacing"));
    }

    #[test]
    fn a_fragment_linted_with_a_file_that_includes_it_is_a_layer() {
        // A fragment's value may replace one of the layers it composes on, so
        // a call that also lints a file including it reads the fragment as a
        // layer: its restated default is not reported and no fix removes it
        // (LAYER-3). Linted alone, the same file is read as standalone.
        let directory = Directory::new("fragments");
        let fragment_text = "yamaa: \"1.0\"\ntype: spec\ninput:\n  AE: {path: ae.csv, \
                             types: {AESEQ: int}, empty_string: missing}\n";
        let fragment = directory.write("shared/fragment.yaml", fragment_text);
        directory.write(
            "adae.yaml",
            "yamaa: \"1.0\"\ntype: spec\ninclude: [shared/fragment.yaml]\n",
        );
        let alone = lint(&[text(&fragment)], false).unwrap();
        assert_eq!(
            alone[0]
                .findings
                .iter()
                .map(|item| (item.finding, item.spec_path.as_str()))
                .collect::<Vec<_>>(),
            [("restated_default", "input.AE.empty_string")]
        );
        let together = lint(&[text(&directory.0)], true).unwrap();
        assert_eq!(
            together
                .iter()
                .map(|file| (file.file.clone(), file.findings.len(), file.fixed))
                .collect::<Vec<_>>(),
            [
                (display(&directory.0.join("adae.yaml")), 0, false),
                (display(&fragment), 0, false),
            ]
        );
        assert_eq!(std::fs::read_to_string(&fragment).unwrap(), fragment_text);
    }

    #[test]
    fn directories_expand_to_every_1_0_file_type_by_its_header() {
        let directory = Directory::new("expand-versioned");
        let broken = VERSIONED.replace("base: DM\n\noutput:", "base: DM\noutput:");
        let specification = directory.write(
            "adae.yaml",
            &CANONICAL.replace("base: DM\n\noutput:", "base: DM\noutput:"),
        );
        let entry = directory.write("adsl.yaml", &broken);
        // A layer declares the type of the chain; its unknown field is the
        // engine's to refuse, not discovery's.
        let layer = directory.write("layers/base.yaml", &format!("{broken}\nunknown: 1\n"));
        let project = directory.write("project.yaml", "yamaa: \"1.0\"\ntype: project\nsdtm: {}\n");
        let function = directory.write(
            "functions/bmi.yaml",
            "yamaa: \"1.0\"\ntype: function\nname: bmi\ntests: []\n",
        );
        let codelists = directory.write(
            "codelists/sponsor.yaml",
            "yamaa: \"1.0\"\ntype: codelists\ncodelists: []\n",
        );
        // A type no file has, no type, and a mapping dictionary, which
        // declares no header, are no yamaa files.
        directory.write("study.yaml", "yamaa: \"1.0\"\ntype: study\nsdtm: {}\n");
        directory.write("untyped.yaml", &broken.replace("type: spec\n", ""));
        directory.write("dictionaries/sex.yaml", "F: Female\nM: Male\n");
        // SCHEMA-2, #3443: directory discovery admits only the 1.x header.
        directory.write(
            "legacy.yaml",
            &CANONICAL.replacen(
                "yamaa: \"1.0\"\ntype: spec\n",
                "schema_version: \"1.0\"\n",
                1,
            ),
        );
        // A root that declares `schema_version` beside `yamaa` is no 1.x
        // file, as the engine reads it.
        directory.write(
            "both.yaml",
            &broken.replace(
                "yamaa: \"1.0\"\n",
                "schema_version: \"1.0\"\nyamaa: \"1.0\"\n",
            ),
        );
        let linted = lint(&[text(&directory.0)], false).unwrap();
        assert_eq!(
            linted
                .iter()
                .map(|file| (
                    file.file.clone(),
                    file.findings
                        .iter()
                        .map(|item| (item.finding, item.spec_path.as_str()))
                        .collect::<Vec<_>>()
                ))
                .collect::<Vec<_>>(),
            [
                (display(&specification), vec![("root_spacing", "output")]),
                (display(&entry), vec![("root_spacing", "output")]),
                (display(&codelists), vec![("root_spacing", "codelists")]),
                (display(&function), vec![("root_spacing", "tests")]),
                (display(&layer), vec![("root_spacing", "output")]),
                (display(&project), vec![("root_spacing", "sdtm")]),
            ]
        );
    }

    #[test]
    fn a_named_file_is_always_linted_and_files_keep_argument_order() {
        let directory = Directory::new("named");
        let clean = directory.write("clean.yaml", CANONICAL);
        // A named file without the 1.x header is still read.
        let notes = directory.write("notes.yaml", "domain: TEST\nstudy: {id: S1}\n");
        let linted = lint(&[text(&notes), text(&clean)], false).unwrap();
        assert_eq!(linted.len(), 2);
        assert_eq!(linted[0].file, display(&notes));
        assert_eq!(linted[0].findings[0].finding, "root_spacing");
        assert!(linted[1].findings.is_empty());
    }

    /// A named file written for 0.5 declares `schema_version`, which no
    /// reader reads: the lint refuses the whole call as
    /// `unsupported_language_version`, naming 0.5 (SCHEMA-2), and writes
    /// nothing, the files named before it included.
    #[test]
    fn a_named_file_that_declares_schema_version_refuses_the_call() {
        let directory = Directory::new("named-0-5");
        let packed = CANONICAL.replace("base: DM\n\noutput:", "base: DM\noutput:");
        let fixable = directory.write("fixable.yaml", &packed);
        let legacy = directory.write(
            "legacy.yaml",
            &CANONICAL.replacen(
                "yamaa: \"1.0\"\ntype: spec\n",
                "schema_version: \"1.0\"\n",
                1,
            ),
        );
        let error = lint(&[text(&fixable), text(&legacy)], true).unwrap_err();
        assert_eq!(error.file(), Some(display(&legacy).as_str()));
        let diagnostics = error.diagnostics();
        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.code,
                    diagnostic.spec_paths.clone(),
                    yamaa_core::json::Value::Object(diagnostic.context.clone()),
                    diagnostic.source_span.is_none() && diagnostic.operand_route.is_none(),
                ))
                .collect::<Vec<_>>(),
            [(
                yamaa_core::diagnostic::ConditionCode::HeaderLanguageVersion,
                vec!["schema_version".to_string()],
                crate::json!({"version": "0.5", "language_versions": ["1.0"]}),
                true,
            )]
        );
        assert_eq!(std::fs::read_to_string(&fixable).unwrap(), packed);
    }

    #[cfg(unix)]
    #[test]
    fn directory_links_are_skipped_and_specification_links_are_refused() {
        use std::os::unix::fs::symlink;

        let directory = Directory::new("links");
        let packed_text = super::packed();
        let packed = directory.write("packed.yaml", &packed_text);
        let target = directory.write(".target/adsl.yaml", CANONICAL);
        symlink(target.parent().unwrap(), directory.0.join("linked.yaml")).unwrap();
        let linted = lint(&[text(&directory.0)], false).unwrap();
        assert_eq!(linted.len(), 1);
        assert_eq!(linted[0].file, display(&packed));

        let link = directory.0.join("linked-file.yaml");
        symlink(&target, &link).unwrap();
        let error = lint(&[text(&directory.0)], true).unwrap_err();
        assert_eq!(error.file(), Some(display(&link).as_str()));
        assert!(matches!(
            error,
            super::super::Error::Read {
                error: crate::file_resources::Error::Symlink,
                ..
            }
        ));
        assert_eq!(std::fs::read_to_string(&packed).unwrap(), packed_text);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), CANONICAL);
    }

    #[cfg(unix)]
    #[test]
    fn directories_skip_special_files_and_links_to_them() {
        use std::os::unix::{
            fs::symlink,
            net::{SocketAddr, UnixListener},
        };

        // A socket's path must fit `sockaddr_un.sun_path` with a closing NUL:
        // 104 bytes on macOS and 108 on Linux. macOS's temporary directory,
        // `/var/folders/<2>/<30>/T`, is 48 bytes and leaves too little room,
        // so this directory goes under `/tmp` there. The socket path is then
        // at most 83 bytes: `/tmp/yamaa-lint-special-files-` (30), a u32
        // process id (10), `-ThreadId(` (10), a u64 thread id (20) and
        // `)/socket.yaml` (13). A path that still does not fit, such as one
        // under a long TMPDIR elsewhere, skips the socket case.
        let root = if cfg!(target_os = "macos") {
            PathBuf::from("/tmp")
        } else {
            std::env::temp_dir()
        };
        let directory = Directory::under(&root, "special-files");
        let packed_text = super::packed();
        let packed = directory.write("packed.yaml", &packed_text);
        let socket = directory.0.join("socket.yaml");
        if let Err(error) = SocketAddr::from_pathname(&socket) {
            eprintln!(
                "{}: {error}; special-file case not exercised",
                socket.display()
            );
            return;
        }
        let _listener = UnixListener::bind(&socket).unwrap();
        symlink(&socket, directory.0.join("linked-socket.yaml")).unwrap();

        for fix in [false, true] {
            let linted = lint(&[text(&directory.0)], fix).unwrap();
            assert_eq!(linted.len(), 1);
            assert_eq!(linted[0].file, display(&packed));
            assert_eq!(linted[0].fixed, fix);
            assert_eq!(
                std::fs::read_to_string(&packed).unwrap(),
                if fix { CANONICAL } else { &packed_text }
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_directories_refuse_before_any_file_is_fixed() {
        use std::os::unix::fs::PermissionsExt;

        for nested in [false, true] {
            let directory = Directory::new("unreadable-directory");
            let packed_text = super::packed();
            let packed = directory.write("packed.yaml", &packed_text);
            directory.write("specs/nested/adsl.yaml", CANONICAL);
            let root = directory.0.join("specs");
            let blocked = if nested {
                root.join("nested")
            } else {
                root.clone()
            };
            let permissions = std::fs::metadata(&blocked).unwrap().permissions();
            std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o0)).unwrap();
            if std::fs::read_dir(&blocked).is_ok() {
                std::fs::set_permissions(&blocked, permissions).unwrap();
                eprintln!("process bypasses DAC; unreadable-directory case not exercised");
                return;
            }
            let results = [false, true].map(|fix| lint(&[text(&packed), text(&root)], fix));
            std::fs::set_permissions(&blocked, permissions).unwrap();
            for result in results {
                let error = result.unwrap_err();
                assert_eq!(error.file(), Some(display(&blocked).as_str()));
                assert!(matches!(
                    error,
                    super::super::Error::Read {
                        error: crate::file_resources::Error::Unreadable,
                        ..
                    }
                ));
                assert_eq!(
                    error.diagnostics(),
                    [yamaa_core::application_issue::rejected(
                        yamaa_core::application_issue::Step::Prepare,
                        yamaa_core::application_issue::Fault::ResourceUnreadable
                    )]
                );
            }
            assert_eq!(std::fs::read_to_string(&packed).unwrap(), packed_text);
        }
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_yaml_candidates_refuse_before_any_file_is_fixed() {
        use std::os::unix::fs::PermissionsExt;

        let directory = Directory::new("unreadable-candidate");
        let packed_text = super::packed();
        let packed = directory.write("packed.yaml", &packed_text);
        let blocked = directory.write("specs/adsl.yaml", CANONICAL);
        let permissions = std::fs::metadata(&blocked).unwrap().permissions();
        std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o0)).unwrap();
        if std::fs::File::open(&blocked).is_ok() {
            std::fs::set_permissions(&blocked, permissions).unwrap();
            eprintln!("process bypasses DAC; unreadable-candidate case not exercised");
            return;
        }
        let results =
            [false, true].map(|fix| lint(&[text(&packed), text(&directory.0.join("specs"))], fix));
        std::fs::set_permissions(&blocked, permissions).unwrap();
        for result in results {
            let error = result.unwrap_err();
            assert_eq!(error.file(), Some(display(&blocked).as_str()));
            assert_eq!(
                error.diagnostics(),
                [yamaa_core::application_issue::rejected(
                    yamaa_core::application_issue::Step::Prepare,
                    yamaa_core::application_issue::Fault::ResourceUnreadable
                )]
            );
        }
        assert_eq!(std::fs::read_to_string(&packed).unwrap(), packed_text);
    }

    #[test]
    fn fix_rewrites_only_the_files_it_proves_and_keeps_their_mode() {
        let directory = Directory::new("fix");
        let packed = directory.write(
            "packed.yaml",
            &CANONICAL.replace(
                "    derivation: DM.STUDYID\n\n",
                "    derivation: DM.STUDYID\n",
            ),
        );
        let clean = directory.write("clean.yaml", CANONICAL);
        // No umask gives a new file an execute bit, so only a kept mode
        // passes.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&packed, std::fs::Permissions::from_mode(0o744)).unwrap();
        }
        let linted = lint(&[text(&packed), text(&clean)], true).unwrap();
        assert_eq!(std::fs::read_to_string(&packed).unwrap(), CANONICAL);
        assert!(linted[0].fixed && linted[0].findings.is_empty());
        assert!(!linted[1].fixed);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&packed).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o744);
        }
        // Nothing staged is left beside the files.
        let mut names = std::fs::read_dir(&directory.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, ["clean.yaml", "packed.yaml"]);
    }

    #[test]
    fn a_file_that_cannot_be_read_or_admitted_refuses_before_anything_is_written() {
        let directory = Directory::new("refuse");
        let packed_text = CANONICAL.replace(
            "    derivation: DM.STUDYID\n\n",
            "    derivation: DM.STUDYID\n",
        );
        let packed = directory.write("packed.yaml", &packed_text);
        let broken = directory.write("broken.yaml", "a: [\n");
        let error = lint(&[text(&packed), text(&broken)], true).unwrap_err();
        assert_eq!(error.file(), Some(display(&broken).as_str()));
        assert!(matches!(error, super::super::Error::Source { .. }));
        assert_eq!(
            error.diagnostics()[0].code,
            ConditionCode::SourceInvalidYaml
        );
        assert_eq!(std::fs::read_to_string(&packed).unwrap(), packed_text);
        let missing = directory.0.join("missing.yaml");
        let error = lint(&[text(&missing)], false).unwrap_err();
        assert!(matches!(error, super::super::Error::Read { .. }));
        let diagnostics = error.diagnostics();
        assert_eq!(diagnostics[0].code, ConditionCode::ResourceMissing);
        assert_eq!(
            diagnostics[0].context.get("path"),
            Some(&yamaa_core::json::Value::String(display(&missing)))
        );
        let ascii = directory.write("accent.yaml", "a: caf\u{e9}\n");
        let error = lint(&[text(&ascii)], false).unwrap_err();
        assert_eq!(error.diagnostics()[0].code, ConditionCode::SourceNonAscii);
    }
}
