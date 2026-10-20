//! The lint's findings and fixes, case by case. Each case writes the departure
//! into a canonical specification and checks the finding, its position, its
//! spec path and message, and what the proved fixes write.

use super::{check_text, fix_text, Finding};

const CANONICAL: &str = r#"schema_version: "1.0"
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
          then: {literal: null}
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
    assert_eq!((first.line, first.column), (17, 1));
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
    let text = r#"schema_version: "1.0"
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
        "schema_version: \"1.0\"\nparent: base.yaml\ndomain: ADSL\nkeys: [USUBJID]\ninput:\n  \
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
        ("entry_spacing", "columns[1]", 25)
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
    let text = "schema_version: \"1.0\"\nname: demo\nnotes: |\n  first\n\n  # not a comment\n  last\nafter: 1\n";
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
        "{impute: {type: date, source: FIRSTDOSE.EXSTDTC, month: 1, day: last, invalid: \
                      {literal: null}}}";
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
    assert!(after.contains("then: {literal: null}"));
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
        [("line_width", label_line, 80, "columns[0].label".to_owned())]
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
fn the_block_form_of_a_literal_is_not_a_finding() {
    // `literal_form` retires with `{literal: X}` in 1.0; the lint names no
    // spelling of a literal.
    for block in [
        "    derivation:\n      literal: DM\n",
        "    derivation:\n      literal: [1, 2]\n",
        "    derivation:\n      literal:\n        DM\n",
        "    derivation:\n      {literal: [1, 2]}\n",
        "    derivation:\n      literal: DM\n      note: x\n",
        "    derivation: {}\n",
    ] {
        let text = CANONICAL.replace("    derivation: DM.USUBJID\n", block);
        assert!(names(&text).is_empty(), "{block}");
    }
}

#[test]
fn source_spellings_are_left_to_the_language() {
    // The language itself refuses {source: X} where the bare string is the
    // shorthand, so the lint names no source spelling.
    for (from, to) in [
        (
            "    derivation: DM.USUBJID\n",
            "    derivation:\n      source: DM.USUBJID\n",
        ),
        (
            "    derivation: DM.USUBJID\n",
            "    derivation:\n      upcase:\n        source: DM.USUBJID\n",
        ),
        (
            "    derivation: DM.STUDYID\n",
            "    derivation:\n      source: {variable: DM.STUDYID, filter: \"DM.X = 1\"}\n",
        ),
    ] {
        assert!(names(&CANONICAL.replace(from, to)).is_empty(), "{to}");
    }
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

const REPEATED: &str = r#"schema_version: "1.0"
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
      DOMAIN: {literal: DM}

  - id: b
    derivations:
      STUDYID: DM.STUDYID
      USUBJID: DM.USUBJID
      DOMAIN: {literal: DM}
"#;

fn repeated(column: &str) -> (&'static str, String) {
    (
        "repeated_row_derivation",
        format!("rows[0].derivations.{column}"),
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
    assert_eq!((found[0].line, found[0].column), (27, 7));
    assert_eq!(
        found[0].message,
        "derivation of `STUDYID` is identical in every row template; move it to columns[].derivation"
    );
    // Authoring lints rewrite the value, so no fix is offered.
    assert_eq!(fixed(REPEATED), REPEATED);
}

#[test]
fn repeated_row_derivation_positions_ignore_stray_row_entries() {
    let stray = REPEATED.replace("rows:\n  - id: a\n", "rows:\n  - stray\n\n  - id: a\n");
    let expected = ["STUDYID", "USUBJID", "DOMAIN"].map(|column| {
        (
            "repeated_row_derivation",
            format!("rows[1].derivations.{column}"),
        )
    });
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
            "      DOMAIN: {literal: DM}\n\n  - id: b",
            "      DOMAIN:\n        aggregate:\n          filter: \"DM.X = 1\"\n          expr: \"ONLY(DM.Y)\"\n\n  - id: b",
        )
        .replace(
            "      DOMAIN: {literal: DM}\n",
            "      DOMAIN:\n        aggregate:\n          filter: \"DM.X = 1\"\n          expr: \"ONLY(DM.Y)\"\n",
        );
    let found = paths(&aggregate);
    assert!(found.contains(&repeated("STUDYID")));
    assert!(!found.contains(&repeated("DOMAIN")));
    let intermediate = REPEATED
        .replace(
            "output:\n  path: dm.csv",
            "intermediates:\n  - id: PICK\n    dataset: DM\n    filter: \"DM.X = 1\"\n    \
             no_match: {literal: null}\n\noutput:\n  path: dm.csv",
        )
        .replace("DOMAIN: {literal: DM}", "DOMAIN: PICK.X");
    assert!(!paths(&intermediate).contains(&repeated("DOMAIN")));
}

#[test]
fn repeated_row_derivation_skips_existing_defaults() {
    let defaulted = REPEATED.replace(
        "  - name: DOMAIN\n    type: str\n    label: Domain Abbreviation\n",
        "  - name: DOMAIN\n    type: str\n    label: Domain Abbreviation\n    derivation: {literal: DM}\n",
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

const PREDICATE_CHAIN: &str = r#"schema_version: "1.0"
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
    type: num
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

const REDUNDANT: &str = r#"schema_version: "1.0"
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  DM: input/dm.csv
  ADSL: input/adsl.csv

output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, AGE]

intermediates:
  - id: ADSL1
    dataset: ADSL
    key: [STUDYID, USUBJID]
    no_match: {literal: null}

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
    derivation: ADSL1.AGE
"#;

#[test]
fn a_redundant_intermediate_reads_the_dataset_directly() {
    let found = findings(REDUNDANT);
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
        [("redundant_intermediate", "intermediates[0]", 13, 5)]
    );
    assert_eq!(
        found[0].message,
        "intermediate `ADSL1` reads exactly what the implicit join to `ADSL` reads; read `ADSL` directly"
    );
    assert_eq!(fixed(REDUNDANT), REDUNDANT);
    // A stray entry keeps the written index of the entries after it.
    let stray = REDUNDANT.replace(
        "intermediates:\n  - id: ADSL1\n",
        "intermediates:\n  - stray\n\n  - id: ADSL1\n",
    );
    assert_eq!(
        paths(&stray),
        [("redundant_intermediate", "intermediates[1]".to_owned())]
    );
}

#[test]
fn a_redundant_intermediate_keeps_narrowing_lookups() {
    let filtered = REDUNDANT.replace(
        "    key: [STUDYID, USUBJID]\n    no_match: {literal: null}",
        "    key: [STUDYID, USUBJID]\n    filter: \"ADSL.EFFFL = 'Y'\"\n    no_match: {literal: null}",
    );
    assert!(names(&filtered).is_empty());
    let custom = REDUNDANT.replace("    key: [STUDYID, USUBJID]", "    key: [USUBJID]");
    assert!(names(&custom).is_empty());
    let required = REDUNDANT.replace("no_match: {literal: null}", "require_match: true");
    assert!(!names(&required).contains(&"redundant_intermediate"));
    // Omitted or cleared by a bare null, the answer is missing, as the join's.
    let omitted = REDUNDANT.replace("\n    no_match: {literal: null}", "");
    assert!(names(&omitted).contains(&"redundant_intermediate"));
    let cleared = REDUNDANT.replace("no_match: {literal: null}", "no_match: null");
    assert!(names(&cleared).contains(&"redundant_intermediate"));
    let answered = REDUNDANT.replace("no_match: {literal: null}", "no_match: {literal: 0}");
    assert!(!names(&answered).contains(&"redundant_intermediate"));
    // An input requiring a match fails its implicit join where the
    // intermediate answers with missing.
    let input_required = REDUNDANT.replace(
        "  ADSL: input/adsl.csv",
        "  ADSL: {path: input/adsl.csv, require_match: true}",
    );
    assert!(!names(&input_required).contains(&"redundant_intermediate"));
    // An omitted key is a rename that validation owns.
    let rename_only = REDUNDANT.replace(
        "    key: [STUDYID, USUBJID]\n    no_match: {literal: null}",
        "    no_match: {literal: null}",
    );
    assert!(!names(&rename_only).contains(&"redundant_intermediate"));
    // A key mapping that restates each output key is the same key.
    let mapped = REDUNDANT.replace(
        "    key: [STUDYID, USUBJID]",
        "    key: {STUDYID: STUDYID, USUBJID: USUBJID}",
    );
    assert!(names(&mapped).contains(&"redundant_intermediate"));
}

#[test]
fn a_redundant_intermediate_is_not_read_by_a_longer_name() {
    // The only qualified read is of the ADSL dataset, whose name merely
    // ends with the intermediate's id.
    let suffix = REDUNDANT
        .replace("  - id: ADSL1\n", "  - id: DSL\n")
        .replace("    derivation: ADSL1.AGE", "    derivation: ADSL.AGE");
    assert!(!names(&suffix).contains(&"redundant_intermediate"));
    let genuine = suffix.replace("    derivation: ADSL.AGE", "    derivation: DSL.AGE");
    assert!(names(&genuine).contains(&"redundant_intermediate"));
    let reads_suffix = REPEATED
        .replace(
            "input:\n  DM: input/dm.csv",
            "input:\n  DM: input/dm.csv\n  ADSL: input/adsl.csv",
        )
        .replace(
            "output:\n  path: dm.csv",
            "intermediates:\n  - id: DSL\n    dataset: DM\n    key: [STUDYID, USUBJID]\n    \
             no_match: {literal: null}\n\noutput:\n  path: dm.csv",
        )
        .replace("DOMAIN: {literal: DM}", "DOMAIN: ADSL.AGE");
    assert!(paths(&reads_suffix).contains(&repeated("DOMAIN")));
}

#[test]
fn qualified_reads_after_escaped_unicode_preserve_identifier_boundaries() {
    // ASCII YAML source can decode to two-, three- or four-byte UTF-8 scalars.
    for escape in [r"\u00e9", r"\u4e2d", r"\U0001f600"] {
        let redundant = REDUNDANT.replace(
            "derivation: ADSL1.AGE",
            &format!("derivation: \"{escape} ADSL1.AGE\""),
        );
        assert_eq!(paths(&redundant), paths(REDUNDANT));
        assert!(!names(&redundant.replace(" ADSL1.AGE", " XADSL1.AGE"))
            .contains(&"redundant_intermediate"));

        let repeated_read = REPEATED
            .replace(
                "columns:\n  - name:",
                "intermediates:\n  - id: PICK\n    dataset: DM\n\ncolumns:\n  - name:",
            )
            .replace(
                "DOMAIN: {literal: DM}",
                &format!("DOMAIN: \"{escape} PICK.X\""),
            );
        assert!(!paths(&repeated_read).contains(&repeated("DOMAIN")));
        assert!(paths(&repeated_read.replace(" PICK.X", " XPICK.X")).contains(&repeated("DOMAIN")));
    }
}

const MISSING_LABEL: &str = r#"schema_version: "1.0"
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
        [("missing_label", "columns[1]", 17, 5)]
    );
    assert_eq!(
        found[0].message,
        "output column `AGE` has no label; declare `label:`"
    );
    assert_eq!(fixed(MISSING_LABEL), MISSING_LABEL);
    // A stray entry keeps the written index; the finding is at the column's
    // own name.
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
        [("columns[2]", 19)]
    );
    // A column the output lists twice has one label departure.
    let repeated = MISSING_LABEL.replace(
        "  columns: [STUDYID, AGE]",
        "  columns: [STUDYID, AGE, AGE]",
    );
    assert_eq!(
        paths(&repeated),
        [("missing_label", "columns[1]".to_owned())]
    );
}

#[test]
fn internal_and_inherited_columns_need_no_label() {
    let internal = MISSING_LABEL.replace("  columns: [STUDYID, AGE]", "  columns: [STUDYID]");
    assert!(names(&internal).is_empty());
    let inherited = MISSING_LABEL.replace(
        "schema_version: \"1.0\"\ndomain: ADSL",
        "schema_version: \"1.0\"\nparent: base.yaml\ndomain: ADSL",
    );
    assert!(!names(&inherited).contains(&"missing_label"));
    // A null parent names no parent: the standalone file owns its labels.
    let standalone = MISSING_LABEL.replace(
        "schema_version: \"1.0\"\ndomain: ADSL",
        "schema_version: \"1.0\"\nparent: null\ndomain: ADSL",
    );
    assert!(names(&standalone).contains(&"missing_label"));
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
            crate::json!({"stage":"prepare", "code":"yaml_reader_unavailable"}),
        ),
        (
            Error::Source {
                file: "spec.yaml".into(),
                failure: crate::yaml_decode::DecodeFailure::Internal,
            },
            crate::json!({"stage":"prepare", "code":"yaml_admission"}),
        ),
        (
            Error::Write {
                file: "spec.yaml".into(),
                error: crate::file_publication::Error::Io(std::io::Error::other("write failed")),
            },
            crate::json!({"stage":"output", "code":"output_publication_failed"}),
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
            crate::json!({"stage":"output", "code":"publication_limit",
                "resource":"output_bytes", "limit":1024, "actual":1025}),
        ),
    ] {
        let diagnostics = error.diagnostics();
        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.code, ConditionCode::ApplicationRejected);
        assert_eq!(diagnostic.requirement(), None);
        assert!(diagnostic.spec_paths.is_empty());
        assert_eq!(Value::Object(diagnostic.context.clone()), context);
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
    }
}

mod files {
    use super::{super::lint, CANONICAL};
    use std::path::{Path, PathBuf};
    use yamaa_core::diagnostic::ConditionCode;

    struct Directory(PathBuf);

    impl Directory {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
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
    fn a_named_file_is_always_linted_and_files_keep_argument_order() {
        let directory = Directory::new("named");
        let clean = directory.write("clean.yaml", CANONICAL);
        let environment = directory.write(
            "environment.yaml",
            "schema_version: \"1.0\"\nstudy: {id: S1}\n",
        );
        let linted = lint(&[text(&environment), text(&clean)], false).unwrap();
        assert_eq!(linted.len(), 2);
        assert_eq!(linted[0].file, display(&environment));
        assert_eq!(linted[0].findings[0].finding, "root_spacing");
        assert!(linted[1].findings.is_empty());
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
        use std::os::unix::{fs::symlink, net::UnixListener};

        let directory = Directory::new("special-files");
        let packed_text = super::packed();
        let packed = directory.write("packed.yaml", &packed_text);
        let socket = directory.0.join("socket.yaml");
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
                        yamaa_core::application_issue::Stage::Prepare,
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
                    yamaa_core::application_issue::Stage::Prepare,
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
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&packed, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        let linted = lint(&[text(&packed), text(&clean)], true).unwrap();
        assert_eq!(std::fs::read_to_string(&packed).unwrap(), CANONICAL);
        assert!(linted[0].fixed && linted[0].findings.is_empty());
        assert!(!linted[1].fixed);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&packed).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o644);
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
