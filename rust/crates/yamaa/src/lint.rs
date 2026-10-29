//! The yamaa file lint (D40, P-LINT-1, P-LINT-2): how a file departs from
//! the written form the style guide fixes, as one table.
//!
//! [`lint`] reads each file it names once, through the local file resolver,
//! and returns the findings that remain as a [`Lint`], whose table has the
//! [`LINT_COLUMNS`]: the file's absolute path with forward slashes, the
//! one-based line and column where the departure starts, the finding (one of
//! [`LINT_FINDINGS`]), its spec path (`$` for the root, in a 1.x file in the
//! notation of the issues table) and a message. Rows come in argument order
//! of the files, a directory's yamaa files of every file type in sorted path
//! order, then by line, column and finding. The result also counts the files
//! linted and names the files a fix rewrote, for the command line's summary
//! line, `N lint findings in M of K files`. A 1.x specification that declares
//! `parent` or `include` is also compared with the layers it inherits, read
//! through the same resolver, for `partial_restatement` and
//! `restated_inherited_value`, which no fix rewrites. A file that declares either, or that a specification of the
//! call includes, itself or through the parent chain it inherits, is a layer,
//! which `restated_default`, `unused_input_key` and `missing_label` do not
//! read.
//!
//! A comment `# yamaa-lint: allow <finding>, ... -- <reason>` directly above a
//! field or an entry suppresses the named findings within it (P-LINT-1); a
//! suppression that names no finding, names one the lint does not report or
//! gives no reason is itself reported as `invalid_suppression`.
//!
//! With `fix`, each file is first rewritten by the proved fixes: it is
//! replaced, atomically, only when the fixed text reads as the same values
//! with the same comments, or for a canonical rewrite of a 1.x file
//! (`restated_default`, `scalar_as_text`, `redundant_long_form`) as those
//! values with exactly that rewrite, and a second pass changes nothing; the
//! table lists what remains. These named files are the only writes outside a
//! specification's declared outputs. A file that cannot be read or admitted
//! as YAML, or a directory that cannot be listed, refuses the whole call with
//! the issue that says why, before any file is written.

use crate::api::Status;
use crate::diagnostic::{ContextValue, Report};
use crate::error::Failure;
use crate::table::Table;
use crate::text::{self, Action};

/// The lint table's columns, in order (P-LINT-2).
pub const LINT_COLUMNS: [&str; 6] = ["file", "line", "column", "finding", "spec_path", "message"];

/// Every value of the lint table's `finding` column.
pub const LINT_FINDINGS: [&str; 16] = yamaa_adapters::lint::FINDINGS;

/// What [`lint`] found: the findings that remain, in table order, with the
/// number of files it read and the files a fix rewrote. The hosts expose its
/// status, [`table`](Lint::table) and rewritten paths; the command line prints each finding's
/// [`text`](LintFinding::text), then the [`summary`](Lint::summary).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lint {
    findings: Vec<LintFinding>,
    files: usize,
    flagged: usize,
    rewritten: Vec<String>,
    fix: bool,
}

/// One departure from the written form: one row of the lint table.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LintFinding {
    /// The file's absolute path, with forward slashes.
    pub file: String,
    /// The one-based line where the departure starts.
    pub line: usize,
    /// The one-based column where the departure starts, in Unicode scalar
    /// values.
    pub column: usize,
    /// One of [`LINT_FINDINGS`].
    pub finding: &'static str,
    /// The spec path of the departure, `$` for the root.
    pub spec_path: String,
    /// The finding's message; not frozen.
    pub message: String,
}

impl LintFinding {
    /// The finding as the command line prints it: `<file>:<line>:<column>:
    /// <finding> <message>`, the file relative to the current directory when
    /// beneath it, and only ASCII.
    pub fn text(&self) -> String {
        let directory = text::current_directory();
        text::ascii(&format!(
            "{}:{}:{}: {} {}",
            text::relative(&self.file, &directory),
            self.line,
            self.column,
            self.finding,
            self.message
        ))
    }

    /// The finding as one JSON object with the lint table's six keys, as the
    /// command line's `--format json` writes it.
    pub fn to_value(&self) -> ContextValue {
        let text = |value: &str| ContextValue::Text(value.to_owned());
        let int = |value: usize| ContextValue::Int(i64::try_from(value).unwrap_or(i64::MAX));
        ContextValue::Map(
            [
                ("file", text(&self.file)),
                ("line", int(self.line)),
                ("column", int(self.column)),
                ("finding", text(self.finding)),
                ("spec_path", text(&self.spec_path)),
                ("message", text(&self.message)),
            ]
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
        )
    }
}

impl Lint {
    /// Failed while any finding remains, succeeded otherwise.
    pub fn status(&self) -> Status {
        if self.findings.is_empty() {
            Status::Succeeded
        } else {
            Status::Failed
        }
    }

    /// The findings that remain, in table order.
    pub fn findings(&self) -> &[LintFinding] {
        &self.findings
    }

    /// The number of files the call linted.
    pub fn files(&self) -> usize {
        self.files
    }

    /// The files a fix rewrote, absolute with forward slashes, in lint order;
    /// empty without `fix`.
    pub fn rewritten(&self) -> &[String] {
        &self.rewritten
    }

    /// The summary line: `<n> lint findings in <m> of <k> files`, counting
    /// the findings that remain, the files that hold them and the files
    /// linted, then `; fixed <f> files` when the call fixed. Counts take the
    /// singular noun for 1.
    pub fn summary(&self) -> String {
        let mut line = format!(
            "{} in {} of {}",
            text::count(self.findings.len(), "lint finding", "lint findings"),
            self.flagged,
            text::count(self.files, "file", "files")
        );
        if self.fix {
            line.push_str("; fixed ");
            line.push_str(&text::count(self.rewritten.len(), "file", "files"));
        }
        line
    }

    /// The lint table (P-LINT-2) with the [`LINT_COLUMNS`], one row per
    /// finding that remains.
    pub fn table(&self) -> Table {
        use yamaa_adapters::public_table::{PublicColumn, PublicTable};

        let int = |cell: fn(&LintFinding) -> usize| {
            PublicColumn::Int(
                self.findings
                    .iter()
                    .map(|item| Some(i64::try_from(cell(item)).unwrap_or(i64::MAX)))
                    .collect(),
            )
        };
        let text = |cell: fn(&LintFinding) -> &str| {
            PublicColumn::Str(self.findings.iter().map(|item| Some(cell(item))).collect())
        };
        let columns = [
            text(|item| &item.file),
            int(|item| item.line),
            int(|item| item.column),
            text(|item| item.finding),
            text(|item| &item.spec_path),
            text(|item| &item.message),
        ];
        let columns = LINT_COLUMNS
            .into_iter()
            .map(str::to_owned)
            .zip(columns)
            .collect();
        Table::from_public(PublicTable::new(columns).expect("the lint columns form one table"))
    }

    fn from_adapter(files: Vec<yamaa_adapters::lint::Linted>, fix: bool) -> Self {
        let flagged = files
            .iter()
            .filter(|file| !file.findings.is_empty())
            .count();
        let rewritten = files
            .iter()
            .filter(|file| file.fixed)
            .map(|file| file.file.clone())
            .collect();
        let count = files.len();
        let findings = files
            .into_iter()
            .flat_map(|linted| {
                let file = linted.file;
                linted.findings.into_iter().map(move |item| LintFinding {
                    file: file.clone(),
                    line: item.line,
                    column: item.column,
                    finding: item.finding,
                    spec_path: item.spec_path,
                    message: item.message,
                })
            })
            .collect();
        Self {
            findings,
            files: count,
            flagged,
            rewritten,
            fix,
        }
    }
}

/// Lint the yamaa files `paths` names, fixing them first when `fix` is true.
///
/// # Example
///
/// ```
/// let path = std::env::temp_dir().join(format!("yamaa-lint-{}.yaml", std::process::id()));
/// std::fs::write(&path, "yamaa: \"1.0\"\ntype: spec\ndomain: DM\noutput:\n  path: dm.csv\n").unwrap();
/// let paths = [path.to_string_lossy().into_owned()];
/// let result = yamaa::lint(&paths, false);
/// if cfg!(feature = "yaml") {
///     let lint = result.unwrap();
///     assert_eq!(lint.findings().len(), 1); // root_spacing: leave one blank line before `output`
///     assert_eq!(lint.summary(), "1 lint finding in 1 of 1 file");
///     assert_eq!(lint.table().columns().len(), yamaa::LINT_COLUMNS.len());
/// } else {
///     assert!(result.is_err());
/// }
/// std::fs::remove_file(&path).unwrap();
/// ```
pub fn lint(paths: &[String], fix: bool) -> Result<Lint, Failure> {
    match yamaa_adapters::lint::lint(paths, fix) {
        Ok(files) => Ok(Lint::from_adapter(files, fix)),
        Err(error) => Err(refusal(&error, paths)),
    }
}

/// D39's refusal, naming the file concerned (relative to the current
/// directory when beneath it), or the first path given when the refusal
/// concerns no file.
fn refusal(error: &yamaa_adapters::lint::Error, paths: &[String]) -> Failure {
    let directory = text::current_directory();
    let path = match error.file() {
        Some(file) => text::relative(file, &directory),
        None => paths.first().map_or("", String::as_str).into(),
    };
    Failure::rejected(
        Action::Lint(&path),
        &Report::from_issues(&error.diagnostics()),
    )
}
