//! The specification lint through the public [`yamaa::lint`] facade: the
//! P-LINT-2 table, file order, fixes, the printed lines and refusals.
#![cfg(feature = "yaml")]

#[cfg(unix)]
use crate::support::scratch;

use yamaa::{lint, Array, DataType, Int64Array, StringArray, Table, LINT_COLUMNS, LINT_FINDINGS};

const CANONICAL: &str = "schema_version: \"1.0\"\ndomain: DM\nkeys: [USUBJID]\ninput:\n  DM: input/dm.csv\n\noutput:\n  path: dm.csv\n  columns: [USUBJID]\n\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Unique Subject Identifier\n    derivation: DM.USUBJID\n";

struct Directory(scratch::Scratch);

impl Directory {
    fn new(name: &str) -> Self {
        Self(scratch::Scratch::new(&format!("lint-{name}")))
    }

    fn write(&self, name: &str, text: &str) -> String {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn absolute(&self, name: &str) -> String {
        std::path::absolute(self.0.join(name))
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
    }
}

/// Each row as (file, line, column, finding, spec path, message).
fn rows(table: &Table) -> Vec<(String, i64, i64, String, String, String)> {
    let mut rows = Vec::new();
    for batch in table.batches() {
        let text = |index: usize| {
            batch
                .column(index)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .clone()
        };
        let integer = |index: usize| {
            batch
                .column(index)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .clone()
        };
        let (file, line, column) = (text(0), integer(1), integer(2));
        let (finding, spec_path, message) = (text(3), text(4), text(5));
        for row in 0..batch.num_rows() {
            assert!(!file.is_null(row) && !line.is_null(row) && !message.is_null(row));
            rows.push((
                file.value(row).to_owned(),
                line.value(row),
                column.value(row),
                finding.value(row).to_owned(),
                spec_path.value(row).to_owned(),
                message.value(row).to_owned(),
            ));
        }
    }
    rows
}

#[test]
fn findings_come_as_the_six_column_table() {
    let directory = Directory::new("table");
    let packed = directory.write(
        "dm.yaml",
        &CANONICAL.replace("input/dm.csv\n\noutput:", "input/dm.csv\noutput:"),
    );
    let table = lint(&[packed], false).unwrap().table();
    let names = table
        .columns()
        .iter()
        .map(|field| field.name().as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, LINT_COLUMNS);
    assert_eq!(
        LINT_COLUMNS,
        ["file", "line", "column", "finding", "spec_path", "message"]
    );
    let types = table
        .columns()
        .iter()
        .map(|field| field.data_type().clone())
        .collect::<Vec<_>>();
    assert_eq!(
        types,
        [
            DataType::Utf8,
            DataType::Int64,
            DataType::Int64,
            DataType::Utf8,
            DataType::Utf8,
            DataType::Utf8,
        ]
    );
    assert_eq!(
        rows(&table),
        [(
            directory.absolute("dm.yaml"),
            6,
            1,
            "root_spacing".to_owned(),
            "output".to_owned(),
            "leave one blank line before `output`".to_owned(),
        )]
    );
    assert!(LINT_FINDINGS.contains(&"root_spacing"));
}

#[test]
fn a_clean_file_gives_an_empty_table() {
    let directory = Directory::new("clean");
    let clean = directory.write("dm.yaml", CANONICAL);
    let linted = lint(&[clean], false).unwrap();
    let table = linted.table();
    assert_eq!(table.rows(), 0);
    assert_eq!(table.columns().len(), LINT_COLUMNS.len());
    assert!(linted.findings().is_empty());
    assert_eq!(linted.summary(), "0 lint findings in 0 of 1 file");
}

#[test]
fn rows_follow_the_files_then_line_column_and_finding() {
    let directory = Directory::new("order");
    let broken = format!(
        "\n{}",
        CANONICAL.replace("input/dm.csv\n\noutput:", "input/dm.csv\noutput:")
    );
    let second = directory.write("b.yaml", &broken);
    let first = directory.write("a.yaml", &broken);
    let table = lint(&[second, first], false).unwrap().table();
    let found = rows(&table)
        .into_iter()
        .map(|(file, line, column, finding, ..)| (file, line, column, finding))
        .collect::<Vec<_>>();
    let expected = |name: &str| {
        [
            (directory.absolute(name), 1, 1, "blank_lines".to_owned()),
            (directory.absolute(name), 7, 1, "root_spacing".to_owned()),
        ]
    };
    assert_eq!(found, [expected("b.yaml"), expected("a.yaml")].concat());
    // A directory names its specifications in sorted path order.
    let table = lint(&[directory.0.to_string_lossy().into_owned()], false)
        .unwrap()
        .table();
    let files = rows(&table)
        .into_iter()
        .map(|(file, ..)| file)
        .collect::<Vec<_>>();
    assert_eq!(
        files,
        [
            directory.absolute("a.yaml"),
            directory.absolute("a.yaml"),
            directory.absolute("b.yaml"),
            directory.absolute("b.yaml"),
        ]
    );
}

#[test]
fn fix_rewrites_the_file_and_lists_what_remains() {
    let directory = Directory::new("fix");
    let written = CANONICAL
        .replace("input/dm.csv\n\noutput:", "input/dm.csv\noutput:")
        .replace(
            "    label: Unique Subject Identifier\n",
            &format!("    label: {}\n", "x".repeat(80)),
        );
    let path = directory.write("dm.yaml", &written);
    let clean = directory.write("clean.yaml", CANONICAL);
    let linted = lint(&[path.clone(), clean], true).unwrap();
    let table = linted.table();
    let fixed = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        fixed,
        CANONICAL.replace(
            "    label: Unique Subject Identifier\n",
            &format!("    label: {}\n", "x".repeat(80)),
        )
    );
    // The label cannot be wrapped, so its finding remains.
    let found = rows(&table)
        .into_iter()
        .map(|(_, line, column, finding, spec_path, _)| (line, column, finding, spec_path))
        .collect::<Vec<_>>();
    assert_eq!(
        found,
        [(
            14,
            80,
            "line_width".to_owned(),
            "columns[0].label".to_owned()
        )]
    );
    // Two files linted, one holds the finding that remains, and the fix
    // rewrote the first, whose path the result names.
    assert_eq!(linted.files(), 2);
    assert_eq!(linted.rewritten(), [directory.absolute("dm.yaml")]);
    assert_eq!(
        linted.summary(),
        "1 lint finding in 1 of 2 files; fixed 1 file"
    );
}

/// The command line's text, authored from `api.md` ("lint()") and the
/// draft's `cli.commands`: one `file:line:column: finding message` line per
/// finding, then `N lint findings in M of K files`.
#[test]
fn findings_print_as_the_style_line_and_the_summary_line() {
    let directory = Directory::new("print");
    let broken = format!(
        "\n{}",
        CANONICAL.replace("input/dm.csv\n\noutput:", "input/dm.csv\noutput:")
    );
    let first = directory.write("a.yaml", &broken);
    let clean = directory.write("clean.yaml", CANONICAL);
    let linted = lint(&[first, clean], false).unwrap();
    let file = directory.absolute("a.yaml");
    let lines = linted
        .findings()
        .iter()
        .map(yamaa::LintFinding::text)
        .collect::<Vec<_>>();
    // The temporary directory is not beneath the current directory, so each
    // file prints absolute.
    assert_eq!(
        lines,
        [
            format!("{file}:1:1: blank_lines remove the leading blank line"),
            format!("{file}:7:1: root_spacing leave one blank line before `output`"),
        ]
    );
    assert_eq!(linted.summary(), "2 lint findings in 1 of 2 files");
    let yamaa::ContextValue::Map(value) = linted.findings()[1].to_value() else {
        panic!("a finding is an object");
    };
    assert_eq!(
        value.keys().map(String::as_str).collect::<Vec<_>>(),
        ["column", "file", "finding", "line", "message", "spec_path"]
    );
    assert_eq!(
        linted.findings()[1].to_value().to_json(),
        format!(
            r#"{{"column":1,"file":"{file}","finding":"root_spacing","line":7,"message":"leave one blank line before `output`","spec_path":"output"}}"#
        )
    );
}

#[test]
fn a_file_that_cannot_be_read_or_parsed_refuses_with_its_issue() {
    let directory = Directory::new("refuse");
    let broken = directory.write("broken.yaml", "a: [\n");
    let failure = lint(&[broken], false).unwrap_err();
    // D39: the call, the first issue's line, and no count when it is the only
    // issue.
    let prefix = format!(
        "cannot lint {}: the file is not valid YAML: ",
        directory.absolute("broken.yaml")
    );
    assert!(
        failure.message.starts_with(&prefix)
            && failure.message.ends_with(" [invalid_yaml, SCHEMA-3]"),
        "{}",
        failure.message
    );
    assert_eq!(failure.diagnostics.len(), 1);
    assert_eq!(
        failure.message,
        format!(
            "cannot lint {}: {}",
            directory.absolute("broken.yaml"),
            failure.diagnostics[0].text()
        )
    );
    let issues = &failure.issues.batches()[0];
    let conditions = issues
        .column(failure.issues.schema().index_of("condition").unwrap())
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    assert_eq!(conditions.value(0), "invalid_yaml");
    let missing = directory
        .0
        .join("missing.yaml")
        .to_string_lossy()
        .into_owned();
    let failure = lint(&[missing], false).unwrap_err();
    let absolute = directory.absolute("missing.yaml");
    assert_eq!(
        failure.message,
        format!(
            "cannot lint {absolute}: {absolute} does not exist [resource_path_missing, FILE-7]"
        )
    );
    let issues = &failure.issues.batches()[0];
    let conditions = issues
        .column(failure.issues.schema().index_of("condition").unwrap())
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    assert_eq!(conditions.value(0), "resource_path_missing");
}

/// A 1.0 file's canonical spellings through the facade: each finding in the
/// table, then `fix` writes the canonical spelling and names the file
/// (P-HND-3, P-KIND-1, P-LONG-1).
#[test]
fn a_1_0_file_is_fixed_to_its_canonical_spellings() {
    const SPELLED: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: DM\nkeys: [USUBJID]\ninput:\n  DM: input/dm.csv\n\noutput:\n  path: dm.csv\n  columns: [USUBJID, AGE]\n\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Unique Subject Identifier\n    derivation: DM.USUBJID\n\n  - name: AGE\n    type: int\n    label: Age\n    derivation: 0\n";
    let written = SPELLED
        .replace(
            "  DM: input/dm.csv\n",
            "  DM: {path: input/dm.csv, relative_to: declaring}\n",
        )
        .replace("derivation: 0\n", "derivation: \"0\"\n");
    let directory = Directory::new("canonical");
    let file = directory.write("dm.yaml", &written);
    let linted = lint(std::slice::from_ref(&file), false).unwrap();
    assert_eq!(
        rows(&linted.table())
            .into_iter()
            .map(|(_, line, column, finding, spec_path, _)| (line, column, finding, spec_path))
            .collect::<Vec<_>>(),
        [
            (
                6,
                28,
                "restated_default".to_owned(),
                "input.DM.relative_to".to_owned()
            ),
            (
                21,
                17,
                "scalar_as_text".to_owned(),
                "columns.AGE.derivation".to_owned()
            ),
        ]
    );
    let fixed = lint(std::slice::from_ref(&file), true).unwrap();
    assert!(fixed.findings().is_empty());
    assert_eq!(fixed.rewritten(), [directory.absolute("dm.yaml")]);
    assert_eq!(
        fixed.summary(),
        "0 lint findings in 0 of 1 file; fixed 1 file"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), SPELLED);
}

/// P-PRINT-1 rule 3 and P-LINT-2: a caller parent alias changes display only.
#[cfg(unix)]
#[test]
fn parent_alias_findings_print_relative_while_fix_keeps_the_written_path() {
    const WRITTEN: &str = "schema_version: \"1.0\"\ndomain: DM\noutput:\n  path: dm.csv\n";
    const FIXED: &str = "schema_version: \"1.0\"\ndomain: DM\n\noutput:\n  path: dm.csv\n";
    let cwd =
        yamaa_adapters::canonical_path::canonicalize(std::env::current_dir().unwrap()).unwrap();
    let study = scratch::Scratch::under(&cwd, "lint-alias-study");
    let aliases = scratch::Scratch::new("lint-alias-caller");
    std::fs::create_dir(study.path().join("nested")).unwrap();
    let alias = aliases.path().join("study");
    let nested_alias = aliases.path().join("nested");
    std::os::unix::fs::symlink(study.path(), &alias).unwrap();
    std::os::unix::fs::symlink(study.path().join("nested"), &nested_alias).unwrap();
    let root = study.path().file_name().unwrap().to_str().unwrap();
    for (given, source, relative, label) in [
        (
            alias.join("spec.yaml"),
            study.path().join("spec.yaml"),
            format!("{root}/spec.yaml"),
            "parent alias",
        ),
        (
            nested_alias.join("spec.yaml"),
            study.path().join("nested/spec.yaml"),
            format!("{root}/nested/spec.yaml"),
            "alias targets nested directory",
        ),
    ] {
        std::fs::write(&source, WRITTEN).unwrap();
        let written = given.to_str().unwrap().to_owned();
        for fix in [false, true] {
            let linted = lint(std::slice::from_ref(&written), fix).unwrap();
            let actual = (
                rows(&linted.table()),
                linted
                    .findings()
                    .iter()
                    .map(yamaa::LintFinding::text)
                    .collect::<Vec<_>>(),
                linted.files(),
                linted.summary(),
                linted.rewritten().to_vec(),
                std::fs::read_to_string(&source).unwrap(),
            );
            let expected = if fix {
                (
                    vec![],
                    vec![],
                    1,
                    "0 lint findings in 0 of 1 file; fixed 1 file".to_owned(),
                    vec![written.clone()],
                    FIXED.to_owned(),
                )
            } else {
                (
                    vec![(
                        written.clone(),
                        3,
                        1,
                        "root_spacing".to_owned(),
                        "output".to_owned(),
                        "leave one blank line before `output`".to_owned(),
                    )],
                    vec![format!(
                        "{relative}:3:1: root_spacing leave one blank line before `output`"
                    )],
                    1,
                    "1 lint finding in 1 of 1 file".to_owned(),
                    vec![],
                    WRITTEN.to_owned(),
                )
            };
            assert_eq!(actual, expected, "{label}, fix={fix}");
        }
    }
}

/// P-PRINT-1 rule 3 and FILE-7: projection preserves a refused path's suffix.
#[cfg(unix)]
#[test]
fn a_parent_alias_refusal_keeps_the_trailing_separator() {
    let cwd =
        yamaa_adapters::canonical_path::canonicalize(std::env::current_dir().unwrap()).unwrap();
    let study = scratch::Scratch::under(&cwd, "lint-alias-refusal");
    let aliases = scratch::Scratch::new("lint-alias-missing-quoted-\"");
    let alias = aliases.path().join("study");
    std::os::unix::fs::symlink(study.path(), &alias).unwrap();
    let written = format!("{}/missing.yaml/", alias.display());
    let relative = format!(
        "{}/missing.yaml/",
        study.path().file_name().unwrap().to_str().unwrap()
    );
    let written_json = yamaa::ContextValue::Text(written.clone()).to_json();
    let message_json = yamaa::ContextValue::Text(format!("{written} does not exist")).to_json();
    for fix in [false, true] {
        let failure = lint(std::slice::from_ref(&written), fix).unwrap_err();
        let actual = (
            failure.message,
            failure
                .diagnostics
                .iter()
                .map(|issue| issue.to_value().to_json())
                .collect::<Vec<_>>(),
            failure.issues.rows(),
            failure.paths,
            std::fs::read_dir(study.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>(),
        );
        let expected = (
            format!(
                "cannot lint {relative}: {written} does not exist [resource_path_missing, FILE-7]"
            ),
            vec![format!(
                r#"{{"column":null,"condition":"resource_path_missing","context":{{"path":{written_json}}},"file":null,"line":null,"message":{message_json},"operand_route":null,"phase":"validation","rule":"FILE-7","severity":"error","source_span":null,"spec_paths":[]}}"#
            )],
            1,
            vec![],
            vec![],
        );
        assert_eq!(
            actual, expected,
            "missing path with trailing separator, fix={fix}"
        );
    }
}
