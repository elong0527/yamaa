//! The specification lint through the public [`yamaa::lint`] facade: the
//! P-LINT-2 table, file order, fixes and refusals.
#![cfg(feature = "yaml")]

use std::path::PathBuf;

use yamaa::{lint, Array, DataType, Int64Array, StringArray, Table, LINT_COLUMNS, LINT_FINDINGS};

const CANONICAL: &str = "schema_version: \"1.0\"\ndomain: DM\nkeys: [USUBJID]\ninput:\n  DM: input/dm.csv\n\noutput:\n  path: dm.csv\n  columns: [USUBJID]\n\ncolumns:\n  - name: USUBJID\n    type: str\n    label: Unique Subject Identifier\n    derivation: DM.USUBJID\n";

struct Directory(PathBuf);

impl Directory {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("yamaa-facade-lint-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
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

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
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
    let table = lint(&[packed], false).unwrap();
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
    let table = lint(&[clean], false).unwrap();
    assert_eq!(table.rows(), 0);
    assert_eq!(table.columns().len(), LINT_COLUMNS.len());
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
    let table = lint(&[second, first], false).unwrap();
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
    let table = lint(&[directory.0.to_string_lossy().into_owned()], false).unwrap();
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
    let table = lint(std::slice::from_ref(&path), true).unwrap();
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
}

#[test]
fn a_file_that_cannot_be_read_or_parsed_refuses_with_its_issue() {
    let directory = Directory::new("refuse");
    let broken = directory.write("broken.yaml", "a: [\n");
    let failure = lint(&[broken], false).unwrap_err();
    assert_eq!(
        failure.message,
        format!(
            "cannot lint {}; see issues",
            directory.absolute("broken.yaml")
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
    let issues = &failure.issues.batches()[0];
    let conditions = issues
        .column(failure.issues.schema().index_of("condition").unwrap())
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    assert_eq!(conditions.value(0), "resource_path_missing");
}
