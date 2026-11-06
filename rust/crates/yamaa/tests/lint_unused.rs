//! `unused_declaration` and `uncalled_function` through the public
//! [`yamaa::lint`] facade: a declaration a specification writes that the
//! resolved specification prunes (LAYER-11), and a function file a project
//! lists that no specification of the project calls once resolved (FUNC-1,
//! FUNC-12). The seam is the variety the shared `lint` host fixture's one
//! case of each leaves out: readers, layers, fragments, copy entries,
//! refused intermediates, suppression and project parents. Each expected row
//! is written by hand: its position is the key of the declaration as the
//! linted file writes it, the first character of a list member it writes,
//! or the name a list holds, and its message is the style guide's.
#![cfg(feature = "yaml")]

use crate::support::scratch::Scratch;

use yamaa::lint;

/// One row of the lint table: (line, column, finding, spec path, message).
type Row = (usize, usize, String, String, String);

/// One table case: its label, the files it writes with their contents, the
/// files the call names, the file whose rows it reads, and those rows.
type Case<T> = (
    &'static str,
    Vec<(&'static str, T)>,
    Vec<&'static str>,
    &'static str,
    Vec<Row>,
);

fn row(line: usize, column: usize, finding: &str, spec_path: &str, message: &str) -> Row {
    (
        line,
        column,
        finding.to_owned(),
        spec_path.to_owned(),
        message.to_owned(),
    )
}

/// The `unused_declaration` row of `spec_path` whose message, after the
/// path, is `rest`.
fn unused(line: usize, column: usize, spec_path: &str, rest: &str) -> Row {
    let message = format!("`{spec_path}` {rest}");
    row(line, column, "unused_declaration", spec_path, &message)
}

/// The `uncalled_function` row of the listing `functions[index]` that holds
/// `written`, the file of `function`.
fn uncalled(line: usize, column: usize, index: usize, function: &str, written: &str) -> Row {
    let path = format!("functions[{index}]");
    let message = format!(
        "`{path}` lists `{function}` ({written}), which no specification of the project calls"
    );
    row(line, column, "uncalled_function", &path, &message)
}

/// The rows of the two findings `lint` gives the file `name` of `directory`
/// when the call names `names`, each of them in that file.
fn rows(directory: &Scratch, names: &[&str], name: &str) -> Vec<Row> {
    let files = names
        .iter()
        .map(|name| directory.join(name).to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let absolute = directory
        .join(name)
        .to_string_lossy()
        .into_owned()
        .replace('\\', "/");
    lint(&files, false)
        .unwrap()
        .findings()
        .iter()
        .filter(|item| item.file == absolute)
        .filter(|item| matches!(item.finding, "unused_declaration" | "uncalled_function"))
        .map(|item| {
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

/// The issue's subject-level specification: `FIRSTEX` is read by nothing,
/// `WEIGHT` only by `WEIGHTBL`, and `WEIGHTBL`, left out of
/// `output.columns`, by nothing.
const ADSL: &str = r#"yamaa: "1.0"
type: spec
model: ADaM
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  DM: {path: ../data/dm.csv, types: {AGE: int}}
  VS: {path: ../data/vs.csv, types: {VSSTRESN: float}}
  EX: {path: ../data/ex.csv, types: {EXSEQ: int}}

output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, AGE]

intermediates:
  - id: FIRSTEX
    dataset: EX
    key: [STUDYID, USUBJID]
    order_by: [EX.EXSEQ]
    keep: first

  - id: WEIGHT
    dataset: VS
    key: [STUDYID, USUBJID]
    filter: "VS.VSTESTCD = 'WEIGHT'"

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

  - name: WEIGHTBL
    type: float
    label: Weight (kg) at Baseline

rows:
  - id: subjects
    dataset: DM
    derivations:
      WEIGHTBL: WEIGHT.VSSTRESN
"#;

/// A template whose own `output.columns` lists all five of its columns.
const TEMPLATE: &str = r#"yamaa: "1.0"
type: spec
model: ADaM
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  DM: {path: ../data/dm.csv, types: {AGE: int}}
  VS: {path: ../data/vs.csv, types: {VSSTRESN: float}}
  EX: {path: ../data/ex.csv, types: {EXSEQ: int}}

output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, AGE, TRTSDT, WEIGHTBL]

intermediates:
  - id: FIRSTEX
    dataset: EX
    key: [STUDYID, USUBJID]
    order_by: [EX.EXSEQ]
    keep: first

  - id: WEIGHT
    dataset: VS
    key: [STUDYID, USUBJID]
    filter: "VS.VSTESTCD = 'WEIGHT'"

columns:
  - {name: STUDYID, type: str, label: Study Identifier, derivation: DM.STUDYID}
  - {name: USUBJID, type: str, label: Unique Subject Identifier, derivation: DM.USUBJID}
  - {name: AGE, type: int, label: Age, derivation: DM.AGE}
  - {name: TRTSDT, type: str, label: First Exposure, derivation: FIRSTEX.EXSTDTC}
  - {name: WEIGHTBL, type: float, label: Weight (kg) at Baseline, derivation: WEIGHT.VSSTRESN}

rows:
  - id: subjects
    dataset: DM
"#;

/// A study layer that keeps four of the template's columns and declares a
/// handedness column it leaves out, with the input and intermediate only
/// that column reads, and relabels the template's `WEIGHTBL`.
const STUDY: &str = r#"yamaa: "1.0"
type: spec
parent: adsl-template.yaml
input:
  SC: ../data/sc.csv

output:
  columns: [STUDYID, USUBJID, AGE, TRTSDT]

intermediates:
  - id: HANDED
    dataset: SC
    key: [STUDYID, USUBJID]
    filter: "SC.SCTESTCD = 'HANDDOM'"

columns:
  - name: WEIGHTBL
    label: Baseline Weight (kg)

  - name: HANDDOM
    type: str
    label: Dominant Hand
    derivation: HANDED.SCSTRESC
"#;

/// Two intermediates read one input, and one pruned column reads another.
const READERS: &str = r#"yamaa: "1.0"
type: spec
domain: ADTOY
keys: [USUBJID]
input:
  DM: dm.csv
  VS: vs.csv
base: DM

output:
  path: out.csv
  columns: [USUBJID]

intermediates:
  - id: HEIGHT
    dataset: VS
    key: [USUBJID]
    filter: "VS.VSTESTCD = 'HEIGHT'"

  - id: WEIGHT
    dataset: VS
    key: [USUBJID]
    filter: "VS.VSTESTCD = 'WEIGHT'"

columns:
  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}
  - {name: WEIGHTKG, type: float, label: Weight, derivation: WEIGHT.VSSTRESN}
  - name: BMI
    type: float
    label: Body Mass Index
    derivation: "WEIGHTKG / POWER(HEIGHT.VSSTRESN / 100, 2)"
"#;

/// An intermediate without `key` that nothing reads, which survives only to
/// reach its refusal (LOOKUP-3), and the input only it reads.
const REFUSED: &str = r#"yamaa: "1.0"
type: spec
domain: ADTOY
keys: [USUBJID]
input:
  DM: dm.csv
  EX: ex.csv
base: DM

output:
  path: out.csv
  columns: [USUBJID]

intermediates:
  - id: FIRSTEX
    dataset: EX

columns:
  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}
"#;

/// A column the study layers list, suppressed with a reason.
const SUPPRESSED: &str = r#"yamaa: "1.0"
type: spec
domain: ADTOY
keys: [USUBJID]
input:
  DM: dm.csv

output:
  path: out.csv
  columns: [USUBJID]

columns:
  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}
  # yamaa-lint: allow unused_declaration -- the study layers list it
  - {name: AGE, type: int, label: Age, derivation: DM.AGE}
"#;

/// A producer whose output contract a copy entry reads (SPEC-9).
const PRODUCER: &str = r#"yamaa: "1.0"
type: spec
domain: ADSL
keys: [USUBJID]
input: {DM: dm.csv}
output:
  path: adsl.csv
  columns: [USUBJID, AGE, SEX]
columns:
  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}
  - {name: AGE, type: int, label: Age, derivation: DM.AGE}
  - {name: SEX, type: str, label: Sex, derivation: DM.SEX}
"#;

/// A consumer that copies two producer columns and outputs one.
const CONSUMER: &str = r#"yamaa: "1.0"
type: spec
domain: ADAE
keys: [USUBJID, AESEQ]
base: AE
input:
  AE: ae.csv
  ADSL: {path: adsl.csv, schema: adsl.yaml, key: [USUBJID]}
output:
  path: adae.csv
  columns: [USUBJID, AESEQ, AGE]
columns:
  - {name: USUBJID, type: str, label: Subject, derivation: AE.USUBJID}
  - {name: AESEQ, type: int, label: Sequence Number, derivation: AE.AESEQ}
  - copy: {dataset: ADSL, columns: [AGE, SEX]}
"#;

/// A specification that includes a fragment declaring a column it leaves
/// out of `output.columns`.
const INCLUDING: &str = r#"yamaa: "1.0"
type: spec
include: [extra.yaml]
domain: ADTOY
keys: [USUBJID]
input:
  DM: dm.csv

output:
  path: out.csv
  columns: [USUBJID]

columns:
  - {name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}
"#;

const FRAGMENT: &str = r#"yamaa: "1.0"
type: spec

columns:
  - {name: EXTRA, type: str, label: Extra, derivation: DM.EXTRA}
"#;

#[test]
fn a_pruned_declaration_is_reported_where_the_file_writes_it() {
    let pruned = "is pruned: it is not in `output.columns` and nothing reads it";
    let cases: Vec<Case<&str>> = vec![
        (
            "each pruned declaration of one file, named with what reads it",
            vec![("adsl.yaml", ADSL)],
            vec!["adsl.yaml"],
            "adsl.yaml",
            vec![
                unused(
                    8,
                    3,
                    "input.VS",
                    "is pruned: only `intermediates.WEIGHT` reads it, which is pruned",
                ),
                unused(
                    9,
                    3,
                    "input.EX",
                    "is pruned: only `intermediates.FIRSTEX` reads it, which is pruned",
                ),
                unused(
                    16,
                    5,
                    "intermediates.FIRSTEX",
                    "is pruned: nothing reads it",
                ),
                unused(
                    22,
                    5,
                    "intermediates.WEIGHT",
                    "is pruned: only `columns.WEIGHTBL` reads it, which is pruned",
                ),
                unused(43, 5, "columns.WEIGHTBL", pruned),
                unused(
                    51,
                    7,
                    "rows.subjects.derivations.WEIGHTBL",
                    "derives `columns.WEIGHTBL`, which is pruned",
                ),
            ],
        ),
        (
            "a layer reports what it writes, an inherited member it writes into among it",
            vec![("adsl-template.yaml", TEMPLATE), ("adsl.yaml", STUDY)],
            vec!["adsl.yaml", "adsl-template.yaml"],
            "adsl.yaml",
            vec![
                unused(
                    5,
                    3,
                    "input.SC",
                    "is pruned: only `intermediates.HANDED` reads it, which is pruned",
                ),
                unused(
                    11,
                    5,
                    "intermediates.HANDED",
                    "is pruned: only `columns.HANDDOM` reads it, which is pruned",
                ),
                unused(
                    17,
                    5,
                    "columns.WEIGHTBL",
                    &format!(
                        "{pruned}; adsl-template.yaml declares it, and this layer writes \
                         `columns.WEIGHTBL.label`"
                    ),
                ),
                unused(20, 5, "columns.HANDDOM", pruned),
            ],
        ),
        (
            "a template whose own output lists every column it declares",
            vec![("adsl-template.yaml", TEMPLATE), ("adsl.yaml", STUDY)],
            vec!["adsl.yaml", "adsl-template.yaml"],
            "adsl-template.yaml",
            vec![],
        ),
        (
            "several readers, intermediates before columns",
            vec![("spec.yaml", READERS)],
            vec!["spec.yaml"],
            "spec.yaml",
            vec![
                unused(
                    7,
                    3,
                    "input.VS",
                    "is pruned: only `intermediates.HEIGHT` and `intermediates.WEIGHT` read \
                     it, which are pruned",
                ),
                unused(
                    15,
                    5,
                    "intermediates.HEIGHT",
                    "is pruned: only `columns.BMI` reads it, which is pruned",
                ),
                unused(
                    20,
                    5,
                    "intermediates.WEIGHT",
                    "is pruned: only `columns.WEIGHTKG` reads it, which is pruned",
                ),
                unused(
                    27,
                    5,
                    "columns.WEIGHTKG",
                    "is pruned: it is not in `output.columns`, and only `columns.BMI` reads \
                     it, which is pruned",
                ),
                unused(28, 5, "columns.BMI", pruned),
            ],
        ),
        (
            "an intermediate that reaches its refusal and the input only it reads",
            vec![("spec.yaml", REFUSED)],
            vec!["spec.yaml"],
            "spec.yaml",
            vec![],
        ),
        (
            "a suppression with a reason",
            vec![("spec.yaml", SUPPRESSED)],
            vec!["spec.yaml"],
            "spec.yaml",
            vec![],
        ),
        (
            "a copied column, at the name its copy entry lists",
            vec![("adsl.yaml", PRODUCER), ("adae.yaml", CONSUMER)],
            vec!["adae.yaml"],
            "adae.yaml",
            vec![unused(15, 42, "columns.SEX", pruned)],
        ),
        (
            "a fragment, and what only the fragment writes in the specification including it",
            vec![("spec.yaml", INCLUDING), ("extra.yaml", FRAGMENT)],
            vec!["spec.yaml", "extra.yaml"],
            "spec.yaml",
            vec![],
        ),
        (
            "the fragment itself",
            vec![("spec.yaml", INCLUDING), ("extra.yaml", FRAGMENT)],
            vec!["spec.yaml", "extra.yaml"],
            "extra.yaml",
            vec![],
        ),
    ];
    for (label, files, names, name, expected) in cases {
        let directory = Scratch::new("lint-unused");
        for (file, contents) in files {
            directory.write(file, contents);
        }
        assert_eq!(rows(&directory, &names, name), expected, "{label}");
    }
}

/// A function file that declares `name`.
fn function(name: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: function\nname: {name}\ndescription: The value itself.\n\
         params: [{{name: x, type: str}}]\nreturns: str\nimplementations: {{python: pkg.{name}}}\n\
         tests:\n  - {{id: normal, covers: [normal], args: {{x: a}}, result: a}}\n  \
         - {{id: empty, covers: [boundary], args: {{x: ''}}, result: ''}}\n  \
         - {{id: missing, covers: ['short-circuit-missing:x'], args: {{x: null}}, result: null}}\n"
    )
}

/// A specification whose `output.columns` are `output` and whose column
/// `ECHOED` calls `echo`.
fn calling(output: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: ADTOY\nkeys: [USUBJID]\ninput:\n  DM: dm.csv\n\n\
         output:\n  path: out.csv\n  columns: [{output}]\n\ncolumns:\n  \
         - {{name: USUBJID, type: str, label: Subject, derivation: DM.USUBJID}}\n  \
         - name: ECHOED\n    type: str\n    label: Echoed\n    derivation:\n      \
         function: {{name: echo, args: {{x: DM.USUBJID}}}}\n"
    )
}

#[test]
fn a_listed_function_no_specification_calls_is_reported_at_its_listing() {
    let header = "yamaa: \"1.0\"\ntype: project\n";
    let listing = "functions: [echo.yaml, shout.yaml]\n";
    let study = "\nadam:\n  specs: [spec.yaml]\n";
    let cases: Vec<Case<String>> = vec![
        (
            "a function the one specification does not call",
            vec![
                ("project.yaml", format!("{header}{listing}{study}")),
                ("spec.yaml", calling("USUBJID, ECHOED")),
            ],
            vec!["project.yaml"],
            "project.yaml",
            vec![uncalled(3, 24, 1, "shout", "shout.yaml")],
        ),
        (
            "a call in a pruned column calls nothing",
            vec![
                ("project.yaml", format!("{header}{listing}{study}")),
                ("spec.yaml", calling("USUBJID")),
            ],
            vec!["project.yaml"],
            "project.yaml",
            vec![
                uncalled(3, 13, 0, "echo", "echo.yaml"),
                uncalled(3, 24, 1, "shout", "shout.yaml"),
            ],
        ),
        (
            "a parent project that lists no specification",
            vec![
                ("base.yaml", format!("{header}{listing}")),
                (
                    "project.yaml",
                    format!("{header}parent: base.yaml\n{study}"),
                ),
                ("spec.yaml", calling("USUBJID, ECHOED")),
            ],
            vec!["base.yaml", "project.yaml"],
            "base.yaml",
            vec![],
        ),
        (
            "a child project that inherits the listing",
            vec![
                ("base.yaml", format!("{header}{listing}")),
                (
                    "project.yaml",
                    format!("{header}parent: base.yaml\n{study}"),
                ),
                ("spec.yaml", calling("USUBJID, ECHOED")),
            ],
            vec!["base.yaml", "project.yaml"],
            "project.yaml",
            vec![],
        ),
        (
            "a child project that writes the listing",
            vec![
                ("base.yaml", format!("{header}{listing}")),
                (
                    "project.yaml",
                    format!("{header}parent: base.yaml\n{listing}{study}"),
                ),
                ("spec.yaml", calling("USUBJID, ECHOED")),
            ],
            vec!["project.yaml"],
            "project.yaml",
            vec![uncalled(4, 24, 1, "shout", "shout.yaml")],
        ),
        (
            "a project whose specification cannot be read",
            vec![("project.yaml", format!("{header}{listing}{study}"))],
            vec!["project.yaml"],
            "project.yaml",
            vec![],
        ),
    ];
    for (label, files, names, name, expected) in cases {
        let directory = Scratch::new("lint-uncalled");
        directory.write("echo.yaml", function("echo"));
        directory.write("shout.yaml", function("shout"));
        for (file, contents) in files {
            directory.write(file, contents);
        }
        assert_eq!(rows(&directory, &names, name), expected, "{label}");
    }
}
