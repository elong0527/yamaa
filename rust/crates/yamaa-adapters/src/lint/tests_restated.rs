//! Restated defaults, spellings and long forms: each departure written into a
//! canonical file, the finding it gives, and the fix that writes it back.

use super::{findings, fixed, names, paths, CODELISTS, FUNCTION, PROJECT};

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

/// A missing source to `str_decimal` gives missing without `missing`
/// (COLUMN-10), so `missing: null` restates that answer, while a written
/// period declares one.
#[test]
fn a_decimal_text_restates_its_missing_answer_as_null() {
    let decimal = |missing: &str| {
        SPELLED.replace(
            "    type: int\n    label: Age\n    derivation: DM.AGE\n",
            &format!(
                "    type: str\n    label: Age\n    derivation:\n      \
                 str_decimal: {{source: DM.AGE, decimals: 1{missing}}}\n"
            ),
        )
    };
    assert_eq!(
        paths(&decimal(", missing: null")),
        [(
            "restated_default",
            "columns.AGE.derivation.str_decimal.missing".to_owned()
        )]
    );
    assert_eq!(fixed(&decimal(", missing: null")), decimal(""));
    assert!(names(&decimal(", missing: '.'")).is_empty());
}

#[test]
fn a_positional_text_verbs_missing_null_restates_its_default() {
    // An omitted `missing` answers a missing source with missing (TEXT-13,
    // TEXT-14), so a written null restates it.
    for (verb, fields) in [
        ("str_substr", "source: DM.AGE, start: 1"),
        ("str_length", "source: DM.AGE"),
    ] {
        let written = |missing: &str| {
            SPELLED.replace(
                "    derivation: DM.AGE\n",
                &format!("    derivation:\n      {verb}: {{{fields}{missing}}}\n"),
            )
        };
        let text = written(", missing: null");
        let path = format!("columns.AGE.derivation.{verb}.missing");
        assert_eq!(
            findings(&text)
                .iter()
                .map(|item| (item.finding, item.spec_path.as_str(), item.message.as_str()))
                .collect::<Vec<_>>(),
            [(
                "restated_default",
                path.as_str(),
                "`missing: null` restates the default; remove it"
            )],
            "{verb}"
        );
        let after = fixed(&text);
        assert_eq!(after, written(""), "{verb}");
        assert!(findings(&after).is_empty(), "{verb}");
    }
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
    // A chain the fixes cannot finish stops where it must: removing the
    // default leaves the input's long form, which validation accepts
    // (SCHEMA-15), but the comment above `path` keeps the shorthand from its
    // key line, so the long form stays and the lint still reports it.
    let stuck = SPELLED.replace(
        "  EX: input/ex.csv\n",
        "  EX:\n    # The exposure records.\n    path: input/ex.csv\n    relative_to: declaring\n",
    );
    assert_eq!(names(&stuck), ["restated_default"]);
    let long_form = stuck.replace("    relative_to: declaring\n", "");
    assert_eq!(fixed(&stuck), long_form);
    assert_eq!(names(&long_form), ["redundant_long_form"]);
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
