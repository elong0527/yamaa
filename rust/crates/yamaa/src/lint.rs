//! The specification lint (D40, P-LINT-1, P-LINT-2): how a file departs from
//! the written form the style guide fixes, as one table.
//!
//! [`lint`] reads each file it names once, through the local file resolver,
//! and returns the findings that remain as a [`Table`] with the
//! [`LINT_COLUMNS`]: the file's absolute path with forward slashes, the
//! one-based line and column where the departure starts, the finding (one of
//! [`LINT_FINDINGS`]), its spec path (`$` for the root) and a message. Rows
//! come in argument order of the files, a directory's specification files in
//! sorted path order, then by line, column and finding.
//!
//! A comment `# yamaa-lint: allow <finding>, ... -- <reason>` directly above a
//! field or an entry suppresses the named findings within it (P-LINT-1); a
//! suppression that names no finding, names one the lint does not report or
//! gives no reason is itself reported as `invalid_suppression`.
//!
//! With `fix`, each file is first rewritten by the proved layout fixes: it is
//! replaced, atomically, only when the fixed text reads as the same values
//! with the same comments and a second pass changes nothing, and the table
//! lists what remains. These named files are the only writes outside a
//! specification's declared outputs. A file that cannot be read or admitted
//! as YAML refuses the whole call with the issue that says why, before any
//! file is written.

use crate::api::Failure;
use crate::diagnostic::Report;
use crate::table::Table;

/// The lint table's columns, in order (P-LINT-2).
pub const LINT_COLUMNS: [&str; 6] = ["file", "line", "column", "finding", "spec_path", "message"];

/// Every value of the lint table's `finding` column.
pub const LINT_FINDINGS: [&str; 9] = yamaa_adapters::lint::FINDINGS;

/// Lint the specification files `paths` names, fixing them first when `fix`
/// is true.
///
/// # Example
///
/// ```
/// let path = std::env::temp_dir().join(format!("yamaa-lint-{}.yaml", std::process::id()));
/// std::fs::write(&path, "schema_version: '1.0'\ndomain: DM\noutput:\n  path: dm.csv\n").unwrap();
/// let paths = [path.to_string_lossy().into_owned()];
/// let result = yamaa::lint(&paths, false);
/// if cfg!(feature = "yaml") {
///     let table = result.unwrap();
///     assert_eq!(table.rows(), 1); // root_spacing: leave one blank line before `output`
///     assert_eq!(table.columns().len(), yamaa::LINT_COLUMNS.len());
/// } else {
///     assert!(result.is_err());
/// }
/// std::fs::remove_file(&path).unwrap();
/// ```
pub fn lint(paths: &[String], fix: bool) -> Result<Table, Failure> {
    match yamaa_adapters::lint::lint(paths, fix) {
        Ok(files) => Ok(table(&files)),
        Err(error) => Err(refusal(&error)),
    }
}

fn refusal(error: &yamaa_adapters::lint::Error) -> Failure {
    let issues = error
        .diagnostics()
        .into_iter()
        .map(|diagnostic| {
            yamaa_adapters::issue_rows::Issue::from_core(diagnostic)
                .expect("a registered finding is portable")
        })
        .collect::<Vec<_>>();
    let message = match error.file() {
        Some(file) => format!("cannot lint {file}; see issues"),
        None => "cannot lint: this build has no YAML reader; see issues".to_owned(),
    };
    Failure::new(message, &Report::from_issues(&issues))
}

fn table(files: &[yamaa_adapters::lint::Linted]) -> Table {
    use arrow_array::builder::{Int64Builder, StringBuilder};
    use arrow_schema::{DataType, Field, Schema};
    use std::sync::Arc;

    let mut file = StringBuilder::new();
    let mut line = Int64Builder::new();
    let mut column = Int64Builder::new();
    let mut finding = StringBuilder::new();
    let mut spec_path = StringBuilder::new();
    let mut message = StringBuilder::new();
    for linted in files {
        for item in &linted.findings {
            file.append_value(&linted.file);
            line.append_value(i64::try_from(item.line).unwrap_or(i64::MAX));
            column.append_value(i64::try_from(item.column).unwrap_or(i64::MAX));
            finding.append_value(item.finding);
            spec_path.append_value(&item.spec_path);
            message.append_value(&item.message);
        }
    }
    let types = [
        DataType::Utf8,
        DataType::Int64,
        DataType::Int64,
        DataType::Utf8,
        DataType::Utf8,
        DataType::Utf8,
    ];
    let schema = Arc::new(Schema::new(
        LINT_COLUMNS
            .into_iter()
            .zip(types)
            .map(|(name, kind)| Field::new(name, kind, true))
            .collect::<Vec<_>>(),
    ));
    let batch = arrow_array::RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(file.finish()),
            Arc::new(line.finish()),
            Arc::new(column.finish()),
            Arc::new(finding.finish()),
            Arc::new(spec_path.finish()),
            Arc::new(message.finish()),
        ],
    )
    .expect("the builders produce exactly the declared lint columns");
    Table::from_public(
        yamaa_adapters::public_table::PublicTable::from_batches(schema, vec![batch])
            .expect("the batch was built against this schema"),
    )
}
