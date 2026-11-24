//! `number_for_text` and `empty_text_comparison`, case by case. The seam is
//! `check_text` and `fix_text` over one specification's text: each case
//! writes the departure into a specification and checks the whole finding,
//! its position, spec path and message, and what the proved fix writes.
//! Positions are counted from the specification text; each message is the
//! one the style guide gives.

use super::{check_text, fix_text, Finding};

/// The zero-padded codes of #4858: numbers written for `str` columns.
const CODES: &str = r#"yamaa: "1.0"
type: spec
model: SDTM
domain: DM
keys: [ID]
input:
  SRC: {path: input/src.csv, types: {ID: int}}

output:
  path: out.csv
  columns: [ID, SITEID, ARMCD]

columns:
  - {name: ID, type: int, label: ID, derivation: SRC.ID}

  - name: SITEID
    type: str
    label: Study Site Identifier
    derivation:
      mapping:
        source: SRC.SITE
        dict: {A: 012, B: 007}

  - name: ARMCD
    type: str
    label: Planned Arm Code
    derivation:
      mapping:
        source: SRC.SITE
        dict: {A: 01, B: 1.50}
"#;

/// The blank test of #4858: comparisons with `''` of a copied input field.
const BLANK: &str = r#"yamaa: "1.0"
type: spec
model: ADaM
domain: ADAE
keys: [USUBJID, AESEQ]
input:
  AE: {path: input/ae.csv, types: {AESEQ: int}}

output:
  path: out.csv
  columns: [USUBJID, AESEQ, AEENDTC, ONGOING]
  report: report.csv

columns:
  - {name: USUBJID, type: str, label: Subject, derivation: AE.USUBJID}
  - {name: AESEQ, type: int, label: Sequence, derivation: AE.AESEQ}
  - {name: AEENDTC, type: str, label: End Date, derivation: AE.AEENDTC}

  - name: ONGOING
    type: str
    label: Ongoing
    derivation:
      case:
        - {when: "AEENDTC = ''", then: "'Y'"}
        - {otherwise: "'N'"}

verifications:
  - row_count: {id: no-blank-end-date, filter: "AEENDTC = ''", max: 0}
"#;

fn findings(text: &str) -> Vec<Finding> {
    check_text(text.as_bytes()).expect("the text is admitted")
}

fn named(text: &str, name: &str) -> Vec<(String, String)> {
    findings(text)
        .into_iter()
        .filter(|item| item.finding == name)
        .map(|item| (item.spec_path, item.message))
        .collect()
}

fn fixed(text: &str) -> String {
    String::from_utf8(fix_text(text.as_bytes()).expect("the text is admitted")).unwrap()
}

fn number(converted: &str, written: &str) -> String {
    format!("a number written for a `str` column converts to `{converted}`; quote it to keep `{written}`")
}

fn blank(read: &str, test: &str, comparison: &str) -> String {
    format!(
        "`{read}` is never an empty string, since an empty field is missing; write \
         `{read} {test}` for `{comparison}`"
    )
}

fn finding(line: usize, column: usize, name: &'static str, path: &str, message: String) -> Finding {
    Finding {
        line,
        column,
        finding: name,
        spec_path: path.into(),
        message,
    }
}

#[test]
fn a_number_written_for_a_text_column_is_quoted_to_keep_its_spelling() {
    let dict = "columns.SITEID.derivation.mapping.dict";
    let arm = "columns.ARMCD.derivation.mapping.dict";
    assert_eq!(
        findings(CODES),
        [
            finding(
                22,
                19,
                "number_for_text",
                &format!("{dict}.A"),
                number("12", "012")
            ),
            finding(
                22,
                27,
                "number_for_text",
                &format!("{dict}.B"),
                number("7", "007")
            ),
            finding(
                30,
                19,
                "number_for_text",
                &format!("{arm}.A"),
                number("1", "01")
            ),
            finding(
                30,
                26,
                "number_for_text",
                &format!("{arm}.B"),
                number("1.5", "1.50")
            ),
        ],
        "the zero-padded codes of #4858"
    );
    let after = CODES
        .replace("{A: 012, B: 007}", r#"{A: "012", B: "007"}"#)
        .replace("{A: 01, B: 1.50}", r#"{A: "01", B: "1.50"}"#);
    assert_eq!(fixed(CODES), after, "fix quotes each dict value");
    assert!(findings(&after).is_empty(), "the fixed file has no finding");
}

#[test]
fn a_number_in_an_expression_field_is_quoted_as_grammar_text() {
    // A case result, a derivation and a handler of a `str` column; the
    // expression fields take a grammar literal and the handler YAML text.
    let cases = [
        (
            "derivation:\n      mapping:\n        source: SRC.SITE\n        dict: {A: 012, B: 007}\n",
            "derivation:\n      case:\n        - {when: \"SRC.SITE = 'A'\", then: 012}\n        - {otherwise: 1.50}\n",
            vec![
                ("columns.SITEID.derivation.case[0].then", number("12", "012")),
                ("columns.SITEID.derivation.case[1].otherwise", number("1.5", "1.50")),
            ],
            "derivation:\n      case:\n        - {when: \"SRC.SITE = 'A'\", then: \"'012'\"}\n        - {otherwise: \"'1.50'\"}\n",
        ),
        (
            "derivation:\n      mapping:\n        source: SRC.SITE\n        dict: {A: 012, B: 007}\n",
            "derivation: 012\n",
            vec![("columns.SITEID.derivation", number("12", "012"))],
            "derivation: \"'012'\"\n",
        ),
        (
            "derivation:\n      mapping:\n        source: SRC.SITE\n        dict: {A: 012, B: 007}\n",
            "derivation:\n      mapping:\n        source: SRC.SITE\n        dict: {A: \"012\"}\n        unmapped: 000\n",
            vec![("columns.SITEID.derivation.mapping.unmapped", number("0", "000"))],
            "derivation:\n      mapping:\n        source: SRC.SITE\n        dict: {A: \"012\"}\n        unmapped: \"000\"\n",
        ),
    ];
    let quoted = CODES.replace("{A: 01, B: 1.50}", r#"{A: "01", B: "1.50"}"#);
    for (old, new, expected, after) in cases {
        let text = quoted.replace(old, new);
        let expected = expected
            .into_iter()
            .map(|(path, message)| (path.to_owned(), message))
            .collect::<Vec<_>>();
        assert_eq!(named(&text, "number_for_text"), expected, "{new}");
        assert_eq!(fixed(&text), quoted.replace(old, after), "{new}");
    }
}

#[test]
fn a_number_that_reads_as_its_text_or_reaches_no_text_column_is_kept() {
    let quoted = CODES.replace("{A: 01, B: 1.50}", r#"{A: "01", B: "1.50"}"#);
    for (old, new) in [
        // Written as the text it converts to.
        ("{A: 012, B: 007}", "{A: 12, B: 1.5}"),
        // Quoted text keeps its spelling.
        ("{A: 012, B: 007}", r#"{A: "012", B: "007"}"#),
        // An `int` column takes the number.
        (
            "    type: str\n    label: Study Site",
            "    type: int\n    label: Study Site",
        ),
    ] {
        assert_eq!(
            named(&quoted.replace(old, new), "number_for_text"),
            [],
            "{new}"
        );
    }
}

#[test]
fn a_comparison_with_the_empty_string_of_a_field_that_never_holds_one_is_reported() {
    let when = "columns.ONGOING.derivation.case[0].when";
    let filter = "verifications[0].row_count.filter";
    assert_eq!(
        findings(BLANK),
        [
            finding(
                24,
                18,
                "empty_text_comparison",
                when,
                blank("AEENDTC", "IS NULL", "AEENDTC = ''")
            ),
            finding(
                28,
                48,
                "empty_text_comparison",
                filter,
                blank("AEENDTC", "IS NULL", "AEENDTC = ''")
            ),
        ],
        "the blank test of #4858"
    );
    assert_eq!(
        fixed(BLANK),
        BLANK,
        "fix leaves the comparison to its author"
    );
    // The input field itself, the other operators and the empty string on
    // the left.
    for (predicate, read, test, comparison) in [
        (
            "AE.AEENDTC = ''",
            "AE.AEENDTC",
            "IS NULL",
            "AE.AEENDTC = ''",
        ),
        ("'' = AEENDTC", "AEENDTC", "IS NULL", "'' = AEENDTC"),
        ("AEENDTC <> ''", "AEENDTC", "IS NOT NULL", "AEENDTC <> ''"),
        (
            "AEENDTC IN ('', 'UNK')",
            "AEENDTC",
            "IS NULL",
            "AEENDTC IN ('', 'UNK')",
        ),
        (
            "AEENDTC NOT IN ('')",
            "AEENDTC",
            "IS NOT NULL",
            "AEENDTC NOT IN ('')",
        ),
        (
            "USUBJID = '001' AND (AEENDTC = '')",
            "AEENDTC",
            "IS NULL",
            "AEENDTC = ''",
        ),
    ] {
        let text = BLANK.replace("\"AEENDTC = ''\", then", &format!("\"{predicate}\", then"));
        assert_eq!(
            named(&text, "empty_text_comparison")[0],
            (when.to_owned(), blank(read, test, comparison)),
            "{predicate}"
        );
    }
}

#[test]
fn a_field_that_can_hold_the_empty_string_is_compared_as_written() {
    let only_filter = |text: &str| {
        named(text, "empty_text_comparison")
            .into_iter()
            .map(|(path, _)| path)
            .collect::<Vec<_>>()
    };
    for (old, new) in [
        // An input that keeps a stored empty string.
        (
            "AE: {path: input/ae.csv, types: {AESEQ: int}}",
            "AE: {path: input/ae.sas7bdat, types: {AESEQ: int}, empty_string: present}",
        ),
        // A column a text operation builds.
        (
            "derivation: AE.AEENDTC}",
            "derivation: \"TRIM(AE.AEENDTC)\"}",
        ),
        // A comparison with a non-empty text.
        ("\"AEENDTC = ''\", then", "\"AEENDTC = ' '\", then"),
    ] {
        let text = BLANK
            .replace(old, new)
            .replace("filter: \"AEENDTC = ''\"", "filter: \"AESEQ > 9\"");
        assert_eq!(only_filter(&text), Vec::<String>::new(), "{new}");
    }
}
