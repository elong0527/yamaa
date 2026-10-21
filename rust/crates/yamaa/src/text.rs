//! Printed text (D37, D39, P-PRINT-1): every line a host or the command line
//! prints for a result, a refusal or a lint, rendered here so that Python's
//! `repr()`, R's `format()` and the command line print the same bytes.
//!
//! - A result prints its [`Summary`]: the summary line, a project run's member
//!   lines, and the first issue's line.
//! - A refusal's message is D39's error text: the [`Action`] that refused, the
//!   first issue's line and the count of the other issues.
//! - An issue prints as one line, `<location>: <message> [<condition>,
//!   <rule>]` ([`crate::Diagnostic::text`]). The location is
//!   `<file>:<line>:<column>`, the file relative to the current directory
//!   when beneath it, and its missing parts are dropped; an issue whose
//!   condition cites no rule prints `[<condition>]`. The command line prints
//!   every issue this way.
//! - A table prints its preview ([`Table::preview`], D43, #3030 D167): the
//!   header `yamaa.Table: <r> rows x <c> columns`, a name row, a type row and
//!   at most 10 rows of at most 8 columns, each cell cut at 20 characters.
//!
//! Every line is ASCII and holds no line break (rule 5). A character outside
//! printable ASCII prints as the escape the YAML reader writes: `\t`, `\n`
//! and `\r`, then `\xHH`, `\uHHHH` or `\UHHHHHHHH` by code point, in lowercase
//! hexadecimal; a backslash prints as itself. Counts are plain integers, with
//! the singular noun for 1, and nothing depends on a terminal.

use std::borrow::Cow;
use std::fmt::{self, Write as _};
use std::ops::Range;
use std::path::Path;

use yamaa_core::temporal::Precision;

use crate::api::Status;
use crate::diagnostic::{Diagnostic, Location};
use crate::table::{
    Array, DataType, Date32Array, Float64Array, Int64Array, LargeStringArray, ListArray,
    RecordBatch, StringArray, StringViewArray, Table, TimeUnit, TimestampSecondArray,
};

/// A result's printed text (D37, P-PRINT-1), one line per field, in print
/// order. Its [`Display`](fmt::Display) form joins the lines with LF and adds
/// no final LF: what Python's `repr()` returns and what R's `format()` splits
/// into lines. The command line prints the summary line and the member
/// lines, then every issue's line instead of `first_issue`.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    /// The summary line, such as `yamaa run of ADAE: succeeded, 1234 rows x 7
    /// columns, 0 issues`.
    pub line: String,
    /// The lines between the summary line and the first issue: a project
    /// run's member lines, in run order, or a selftest's `first failure:`,
    /// `not covered:` and information lines; empty for any other result.
    pub members: Vec<String>,
    /// `first issue: <issue line>`, when the result has an issue.
    pub first_issue: Option<String>,
}

impl Summary {
    /// A lazy specification object, which has read nothing: `yamaa spec
    /// <path>`, with the path as given.
    pub fn spec(path: &str) -> Self {
        Self::lazy("spec", path)
    }

    /// A lazy project object, which has read nothing: `yamaa project
    /// <path>`, with the path as given.
    pub fn project(path: &str) -> Self {
        Self::lazy("project", path)
    }

    fn lazy(kind: &str, path: &str) -> Self {
        Self {
            line: ascii(&format!("yamaa {kind} {path}")),
            members: Vec::new(),
            first_issue: None,
        }
    }

    /// The lines in print order: the summary line, the member lines, then
    /// the first issue's line.
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.line.as_str())
            .chain(self.members.iter().map(String::as_str))
            .chain(self.first_issue.as_deref())
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, line) in self.lines().enumerate() {
            if index > 0 {
                f.write_char('\n')?;
            }
            f.write_str(line)?;
        }
        Ok(())
    }
}

/// The call a refusal names first in its error text (D39): one of a closed
/// set, since a failed run's `save()` refuses with the run's own issues and
/// the first message alone would not say which call refused. Its
/// [`Display`](fmt::Display) form is the prefix, such as `cannot save ADAE`.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action<'a> {
    /// `save()` of the result whose summary names this subject: a run's
    /// dataset name, or a project path with its section; or
    /// [`crate::Engine::save_table`] to this path, as given.
    Save(&'a str),
    /// `resolve()` of the specification at this path, as given.
    Resolve(&'a str),
    /// Selecting this name: a project's specification, a project run's
    /// member, or a section.
    Select(&'a str),
    /// Reading the file at this path, as given.
    Read(&'a str),
    /// Linting the file at this path.
    Lint(&'a str),
    /// The JSON Schema of this file type.
    Schema(&'a str),
}

impl fmt::Display for Action<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Save(subject) => write!(f, "cannot save {subject}"),
            Self::Resolve(path) => write!(f, "cannot resolve {path}"),
            Self::Select(name) => write!(f, "cannot select {name}"),
            Self::Read(path) => write!(f, "cannot read {path}"),
            Self::Lint(path) => write!(f, "cannot lint {path}"),
            Self::Schema(file_type) => write!(f, "cannot give the schema of {file_type}"),
        }
    }
}

/// D39's error text: `<action>: <first issue line> (and <n> more issues)`,
/// or `<action>: <detail>` for a refusal that carries no issue, where
/// `detail` says what failed.
pub(crate) fn error_text(
    action: Action<'_>,
    issues: &[Diagnostic],
    detail: Option<&str>,
) -> String {
    let directory = current_directory();
    let mut text = action.to_string();
    match issues.split_first() {
        Some((first, others)) => {
            let _ = write!(text, ": {}", issue_line(first, &directory));
            if !others.is_empty() {
                let noun = if others.len() == 1 { "issue" } else { "issues" };
                let _ = write!(text, " (and {} more {noun})", others.len());
            }
        }
        None => {
            if let Some(detail) = detail {
                let _ = write!(text, ": {detail}");
            }
        }
    }
    ascii(&text)
}

/// What a result's summary line says about the result.
pub(crate) struct Head<'a> {
    /// `check`, `run`, `project run` or `submission`.
    pub(crate) kind: &'static str,
    pub(crate) subject: &'a str,
    pub(crate) status: Status,
    /// The size; a check, which reads no data, has none.
    pub(crate) size: Option<String>,
}

/// One member line of a project run's summary.
pub(crate) struct MemberLine {
    pub(crate) name: String,
    pub(crate) status: Status,
    pub(crate) size: String,
    pub(crate) issues: usize,
}

/// A result's summary: `yamaa <kind> of <subject>: <status>[, <size>], <n>
/// issues`, then the member lines, then the first of `issues`.
pub(crate) fn summary(head: Head<'_>, members: &[MemberLine], issues: &[Diagnostic]) -> Summary {
    let mut line = format!(
        "yamaa {} of {}: {}",
        head.kind,
        head.subject,
        head.status.as_str()
    );
    if let Some(size) = head.size {
        let _ = write!(line, ", {size}");
    }
    let _ = write!(line, ", {}", count(issues.len(), "issue", "issues"));
    let directory = current_directory();
    Summary {
        line: ascii(&line),
        members: member_lines(members),
        first_issue: issues
            .first()
            .map(|issue| format!("first issue: {}", issue_line(issue, &directory))),
    }
}

/// Two spaces, the name padded to the longest name, the status padded to 9
/// characters, then the size and the issue count, each column two spaces
/// apart (rule 4).
fn member_lines(members: &[MemberLine]) -> Vec<String> {
    let names = members
        .iter()
        .map(|member| ascii(&member.name))
        .collect::<Vec<_>>();
    let width = names.iter().map(String::len).max().unwrap_or(0);
    members
        .iter()
        .zip(names)
        .map(|(member, name)| {
            format!(
                "  {name:<width$}  {:<9}  {}, {}",
                member.status.as_str(),
                ascii(&member.size),
                count(member.issues, "issue", "issues")
            )
        })
        .collect()
}

/// The size of a run: `<r> rows x <c> columns`, or `no output`.
pub(crate) fn output_size(output: Option<&Table>) -> String {
    match output {
        Some(table) => format!(
            "{} x {}",
            count(table.rows(), "row", "rows"),
            count(table.columns().len(), "column", "columns")
        ),
        None => "no output".to_owned(),
    }
}

/// The size of a project run: `<k> specifications`.
pub(crate) fn specifications(members: usize) -> String {
    count(members, "specification", "specifications")
}

/// The size of a submission: `<d> define.xml and <t> transport files`.
pub(crate) fn submission_size(defines: usize, transports: usize) -> String {
    format!(
        "{defines} define.xml and {}",
        count(transports, "transport file", "transport files")
    )
}

/// One issue's printed line (rule 3), the location relative to the current
/// directory, whose spellings `directory` holds ([`current_directory`]).
pub(crate) fn issue_line(diagnostic: &Diagnostic, directory: &[String]) -> String {
    let mut line = String::new();
    if let Some(location) = &diagnostic.location {
        let _ = write!(line, "{}: ", located(location, directory));
    }
    let _ = write!(line, "{} [{}", diagnostic.message, diagnostic.code);
    if let Some(rule) = &diagnostic.rule {
        let _ = write!(line, ", {rule}");
    }
    line.push(']');
    ascii(&line)
}

/// `<file>:<line>:<column>`, or only the file when no node was located.
fn located(location: &Location, directory: &[String]) -> String {
    let file = relative(&location.file, directory);
    match location.position {
        Some(position) => format!("{file}:{}:{}", position.line, position.column),
        None => file.to_owned(),
    }
}

/// `file` relative to the current directory when the file lies beneath it,
/// and as it is otherwise. `directory` holds the current directory's
/// spellings ([`current_directory`]); the file and each spelling are
/// absolute, with forward slashes.
pub(crate) fn relative<'a>(file: &'a str, directory: &[String]) -> &'a str {
    directory
        .iter()
        .find_map(|spelling| {
            file.strip_prefix(spelling.trim_end_matches('/'))?
                .strip_prefix('/')
                .filter(|rest| !rest.is_empty())
        })
        .unwrap_or(file)
}

/// The current directory in each spelling a printed file can start with,
/// absolute with forward slashes: first its identity, canonical as
/// compilation spells each file it reads and so the file of an issue's
/// location; then the spelling the operating system reports, which starts a
/// lint finding's file, the path given made absolute. The two differ when the
/// directory was entered by another spelling, such as Windows 8.3 short
/// names, so a located file prints relative to the current directory however
/// that is spelled (#3540). Empty when the current directory cannot be read.
pub(crate) fn current_directory() -> Vec<String> {
    std::env::current_dir().map_or_else(|_| Vec::new(), |directory| spellings(&directory))
}

/// The spellings of the existing directory `directory`: its identity, then
/// the spelling given, each when it can be read as text.
fn spellings(directory: &Path) -> Vec<String> {
    yamaa_adapters::file_configuration::directory_identity(directory)
        .ok()
        .into_iter()
        .chain(directory.to_str().map(|given| given.replace('\\', "/")))
        .collect()
}

/// `n` and the noun, singular for 1.
pub(crate) fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `text` with every character outside printable ASCII escaped (rule 5).
pub(crate) fn ascii(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        escape(&mut out, character);
    }
    out
}

/// Append `character` to `out`, escaped unless it is printable ASCII.
fn escape(out: &mut String, character: char) {
    match character {
        ' '..='~' => out.push(character),
        '\t' => out.push_str("\\t"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        other => {
            let point = u32::from(other);
            let _ = match point {
                0..=0xff => write!(out, "\\x{point:02x}"),
                0x100..=0xffff => write!(out, "\\u{point:04x}"),
                _ => write!(out, "\\U{point:08x}"),
            };
        }
    }
}

/// The most value rows a table's preview prints: a longer table prints the
/// first half and the last half of them, around a `...` line.
const PREVIEW_ROWS: usize = 10;
/// The most columns a table's preview prints, the table's first ones.
const PREVIEW_COLUMNS: usize = 8;
/// The most characters one cell of a table's preview prints.
const CELL_WIDTH: usize = 20;
/// What marks the rows and the columns a preview leaves out, and ends a cut
/// cell.
const ELIDED: &str = "...";

/// A table's printed preview (D43, P-PRINT-1, #3030 D167): the header, then,
/// when the table has columns, its name row, its type row and its value rows
/// ([`Table::preview`]).
pub(crate) fn preview(table: &Table) -> String {
    let header = format!("yamaa.Table: {}", output_size(Some(table)));
    let fields = table.columns();
    if fields.is_empty() {
        return header;
    }
    let shown = fields.len().min(PREVIEW_COLUMNS);
    let rows = table.rows();
    let (head, tail) = if rows > PREVIEW_ROWS {
        (PREVIEW_ROWS / 2, rows - PREVIEW_ROWS / 2)
    } else {
        (rows, rows)
    };
    // The cells of each line below the header; `None` is the `...` line.
    let mut grid = vec![
        Some(
            fields
                .iter()
                .take(shown)
                .map(|field| cell(field.name()))
                .collect::<Vec<_>>(),
        ),
        Some(
            fields
                .iter()
                .take(shown)
                .map(|field| cell(&type_name(field.data_type())))
                .collect(),
        ),
    ];
    let values = |range: Range<usize>| {
        rows_of(table.batches(), range).map(|(batch, row)| {
            Some(
                batch
                    .columns()
                    .iter()
                    .take(shown)
                    .map(|array| cell(&value(array.as_ref(), row)))
                    .collect(),
            )
        })
    };
    grid.extend(values(0..head));
    if tail > head {
        grid.push(None);
    }
    grid.extend(values(tail..rows));
    let widths = (0..shown)
        .map(|column| {
            grid.iter()
                .flatten()
                .map(|cells| cells.get(column).map_or(0, String::len))
                .max()
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    let mut lines = vec![header];
    lines.extend(grid.iter().map(|cells| match cells {
        Some(cells) => laid_out(cells, &widths, fields.len() > shown),
        None => ELIDED.to_owned(),
    }));
    lines.join("\n")
}

/// The batch, and the row within it, of each table row in `range`, in order.
fn rows_of(
    batches: &[RecordBatch],
    range: Range<usize>,
) -> impl Iterator<Item = (&RecordBatch, usize)> {
    let mut first = 0;
    batches.iter().flat_map(move |batch| {
        let start = first;
        first += batch.num_rows();
        (range.start.max(start)..range.end.min(first)).map(move |row| (batch, row - start))
    })
}

/// One line of a preview: each cell padded to its column's width, the
/// columns two spaces apart, then the `...` column when columns were left
/// out. No line ends in a space.
fn laid_out(cells: &[String], widths: &[usize], elided: bool) -> String {
    let mut line = String::new();
    for (index, (cell, width)) in cells.iter().zip(widths).enumerate() {
        if index > 0 {
            line.push_str("  ");
        }
        let _ = write!(line, "{cell:<width$}");
    }
    if elided {
        line.push_str("  ");
        line.push_str(ELIDED);
    }
    line.truncate(line.trim_end_matches(' ').len());
    line
}

/// `text` as one cell of a preview, in ASCII with the escapes of rule 5.
/// When that prints more than 20 characters, the cell keeps the whole
/// characters that print at most 17, never part of an escape, then `...`.
fn cell(text: &str) -> String {
    let mut out = String::new();
    let mut fits = 0;
    for character in text.chars() {
        escape(&mut out, character);
        if out.len() > CELL_WIDTH {
            out.truncate(fits);
            out.push_str(ELIDED);
            return out;
        }
        if out.len() <= CELL_WIDTH - ELIDED.len() {
            fits = out.len();
        }
    }
    out
}

/// The language name of a column's type: `str`, `int`, `float`, `date`,
/// `datetime`, or `list<T>` for a list of `T`, and `?` for a type no engine
/// table has. A column may hold missing values whatever its type, so no name
/// says `optional`.
fn type_name(kind: &DataType) -> String {
    match kind {
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => "str".to_owned(),
        DataType::Int64 => "int".to_owned(),
        DataType::Float64 => "float".to_owned(),
        DataType::Date32 => "date".to_owned(),
        DataType::Timestamp(TimeUnit::Second, None) => "datetime".to_owned(),
        DataType::List(item) => format!("list<{}>", type_name(item.data_type())),
        _ => "?".to_owned(),
    }
}

/// The text of the value at `row` of `array`: `null` when it is missing, a
/// float in VALUE-8's canonical text (a nonfinite float is missing, as at the
/// scalar boundary), a date or datetime in VALUE-17's, and a list as its
/// items' texts joined by `, ` in brackets. A value the preview cannot print,
/// of a type no engine table has, prints `?`.
fn value(array: &dyn Array, row: usize) -> Cow<'_, str> {
    if array.is_null(row) {
        return Cow::Borrowed("null");
    }
    let any = array.as_any();
    let text = match array.data_type() {
        DataType::Utf8 => any
            .downcast_ref::<StringArray>()
            .map(|values| Cow::Borrowed(values.value(row))),
        DataType::LargeUtf8 => any
            .downcast_ref::<LargeStringArray>()
            .map(|values| Cow::Borrowed(values.value(row))),
        DataType::Utf8View => any
            .downcast_ref::<StringViewArray>()
            .map(|values| Cow::Borrowed(values.value(row))),
        DataType::Int64 => any
            .downcast_ref::<Int64Array>()
            .map(|values| Cow::Owned(values.value(row).to_string())),
        DataType::Float64 => any.downcast_ref::<Float64Array>().map(|values| {
            yamaa_core::value::FiniteFloat::new(values.value(row))
                .map_or(Cow::Borrowed("null"), |value| {
                    Cow::Owned(yamaa_core::conversion::float_text(value))
                })
        }),
        DataType::Date32 => any
            .downcast_ref::<Date32Array>()
            .and_then(|values| {
                yamaa_adapters::arrow_temporal::date_from_days(
                    i64::from(values.value(row)),
                    Precision::Day,
                )
            })
            .map(|date| Cow::Owned(date.to_string())),
        DataType::Timestamp(TimeUnit::Second, None) => any
            .downcast_ref::<TimestampSecondArray>()
            .and_then(|values| {
                yamaa_adapters::arrow_temporal::datetime_from_seconds(
                    values.value(row),
                    Precision::Second,
                )
            })
            .map(|datetime| Cow::Owned(datetime.to_string())),
        DataType::List(_) => any.downcast_ref::<ListArray>().map(|lists| {
            let items = lists.value(row);
            let texts = (0..items.len())
                .map(|item| value(items.as_ref(), item))
                .collect::<Vec<_>>();
            Cow::Owned(format!("[{}]", texts.join(", ")))
        }),
        _ => None,
    };
    text.unwrap_or(Cow::Borrowed("?"))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow_array::builder::{Int64Builder, ListBuilder, StringBuilder};
    use arrow_array::{ArrayRef, BooleanArray, RecordBatchOptions};
    use arrow_schema::Schema;
    use yamaa_adapters::public_table::PublicTable;

    use super::*;
    use crate::diagnostic::Position;
    use crate::table::Field;

    #[test]
    fn printable_ascii_passes_and_everything_else_is_escaped() {
        assert_eq!(ascii("ADSL a/b\\c [x, y] ~"), "ADSL a/b\\c [x, y] ~");
        assert_eq!(
            ascii("one\ttwo\nthree\rfour\u{0}\u{7f}"),
            "one\\ttwo\\nthree\\rfour\\x00\\x7f"
        );
        assert_eq!(ascii("é ß"), "\\xe9 \\xdf");
        assert_eq!(ascii("İ €"), "\\u0130 \\u20ac");
        assert_eq!(ascii("🙂"), "\\U0001f642");
        // Escaped text is printable ASCII, so escaping twice changes nothing.
        let once = ascii("é\n🙂");
        assert_eq!(ascii(&once), once);
    }

    #[test]
    fn counts_take_the_singular_for_one() {
        assert_eq!(count(0, "issue", "issues"), "0 issues");
        assert_eq!(count(1, "issue", "issues"), "1 issue");
        assert_eq!(count(2, "issue", "issues"), "2 issues");
        assert_eq!(specifications(1), "1 specification");
        assert_eq!(
            submission_size(1, 12),
            "1 define.xml and 12 transport files"
        );
        assert_eq!(submission_size(2, 1), "2 define.xml and 1 transport file");
        assert_eq!(output_size(None), "no output");
    }

    fn spelled(spellings: &[&str]) -> Vec<String> {
        spellings
            .iter()
            .map(|&spelling| spelling.to_owned())
            .collect()
    }

    #[test]
    fn a_file_beneath_the_directory_prints_relative_to_it() {
        let directory = spelled(&["/study"]);
        assert_eq!(
            relative("/study/adam/adae.yaml", &directory),
            "adam/adae.yaml"
        );
        assert_eq!(
            relative("/study/adam/adae.yaml", &spelled(&["/study/"])),
            "adam/adae.yaml"
        );
        assert_eq!(
            relative("/studyx/adae.yaml", &directory),
            "/studyx/adae.yaml"
        );
        assert_eq!(relative("/other/adae.yaml", &directory), "/other/adae.yaml");
        assert_eq!(relative("/study", &directory), "/study");
        assert_eq!(relative("/study/adae.yaml", &[]), "/study/adae.yaml");
        assert_eq!(
            relative("C:/study/dm.yaml", &spelled(&["C:/study"])),
            "dm.yaml"
        );
        // A file beneath any spelling of the directory, its identity or the
        // spelling the system reports, prints relative to it.
        let directory = spelled(&["C:/Users/runneradmin/study/", "C:/Users/RUNNER~1/study"]);
        assert_eq!(
            relative("C:/Users/runneradmin/study/adam/adae.yaml", &directory),
            "adam/adae.yaml"
        );
        assert_eq!(
            relative("C:/Users/RUNNER~1/study/dm.yaml", &directory),
            "dm.yaml"
        );
        assert_eq!(
            relative("C:/Users/runneradmin/studyx/dm.yaml", &directory),
            "C:/Users/runneradmin/studyx/dm.yaml"
        );
    }

    /// A located issue's file is canonical, as compilation spells it, and a
    /// lint finding's file is the path given, made absolute: each prints
    /// relative to the current directory however the directory is spelled.
    #[test]
    fn a_file_prints_relative_to_every_spelling_of_its_directory() {
        // The temporary directory as the environment spells it: on Windows
        // runners an 8.3 short path, such as `C:\Users\RUNNER~1\...`.
        let directory =
            std::env::temp_dir().join(format!("yamaa-text-spellings-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let canonical = yamaa_adapters::canonical_path::canonicalize(&directory).unwrap();
        let mut spellings_of = vec![directory.clone(), canonical.clone()];
        #[cfg(unix)]
        let link = directory.with_extension("link");
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(&directory, &link).unwrap();
            spellings_of.push(link.clone());
        }
        #[cfg(windows)]
        {
            // Another drive-letter case, every name in capitals, and the
            // verbatim prefix.
            let text = canonical.to_str().unwrap();
            let (drive, rest) = text.split_at(1);
            spellings_of.extend(
                [
                    format!("{}{rest}", drive.to_ascii_lowercase()),
                    text.to_uppercase(),
                    format!(r"\\?\{text}"),
                ]
                .map(std::path::PathBuf::from),
            );
        }
        let located = yamaa_adapters::file_configuration::caller_identity(
            directory.join("adae.yaml").to_str().unwrap(),
        )
        .unwrap();
        let printed = spellings_of
            .iter()
            .map(|spelling| {
                let directory = spellings(spelling);
                let given = format!(
                    "{}/adae.yaml",
                    spelling.to_str().unwrap().replace('\\', "/")
                );
                [
                    relative(&located, &directory).to_owned(),
                    relative(&given, &directory).to_owned(),
                ]
            })
            .collect::<Vec<_>>();
        #[cfg(unix)]
        std::fs::remove_file(&link).unwrap();
        std::fs::remove_dir_all(&directory).unwrap();
        for (spelling, printed) in spellings_of.iter().zip(printed) {
            assert_eq!(printed, ["adae.yaml"; 2], "{}", spelling.display());
        }
    }

    #[test]
    fn a_location_drops_its_missing_parts() {
        let file = |position| Location {
            file: "/study/adam/adae.yaml".to_owned(),
            position,
        };
        let at = Some(Position {
            line: 57,
            column: 9,
        });
        let study = spelled(&["/study"]);
        assert_eq!(located(&file(at), &study), "adam/adae.yaml:57:9");
        assert_eq!(located(&file(None), &study), "adam/adae.yaml");
        assert_eq!(
            located(&file(at), &spelled(&["/elsewhere"])),
            "/study/adam/adae.yaml:57:9"
        );
    }

    #[test]
    fn every_action_names_its_call() {
        let prefixes = [
            (Action::Save("ADAE"), "cannot save ADAE"),
            (
                Action::Resolve("adam/adae.yaml"),
                "cannot resolve adam/adae.yaml",
            ),
            (Action::Select("ADSL"), "cannot select ADSL"),
            (Action::Read("edc/odm.xml"), "cannot read edc/odm.xml"),
            (Action::Lint("/study/dm.yaml"), "cannot lint /study/dm.yaml"),
            (Action::Schema("spec"), "cannot give the schema of spec"),
        ];
        for (action, prefix) in prefixes {
            assert_eq!(action.to_string(), prefix);
            assert_eq!(error_text(action, &[], None), prefix);
            assert_eq!(
                error_text(action, &[], Some("why")),
                format!("{prefix}: why")
            );
        }
    }

    /// One issue has no count; two say `(and 1 more issue)`; an issue whose
    /// condition cites no rule prints only its condition in brackets.
    #[test]
    fn error_text_counts_the_other_issues() {
        use yamaa_core::diagnostic::ConditionCode;
        let issue = |code: ConditionCode| yamaa_core::diagnostic::Diagnostic {
            code,
            spec_paths: vec!["columns.A".into()],
            context: [(
                "path".to_owned(),
                yamaa_core::json::Value::String("in/é.csv".into()),
            )]
            .into(),
            source_span: None,
            operand_route: None,
        };
        let report = crate::diagnostic::Report::from_issues(&[
            issue(ConditionCode::ResourceMissing),
            issue(ConditionCode::ApplicationRejected),
        ]);
        let [missing, rejected] = report.diagnostics.as_slice() else {
            panic!("two diagnostics");
        };
        let missing_line = issue_line(missing, &[]);
        assert!(missing_line.ends_with(&format!(
            " [{}, {}]",
            missing.code,
            missing.rule.as_deref().unwrap()
        )));
        // The message's non-ASCII value prints escaped.
        assert!(missing_line.contains("in/\\xe9.csv"), "{missing_line}");
        assert!(issue_line(rejected, &[]).ends_with(&format!(" [{}]", rejected.code)));
        assert_eq!(
            error_text(Action::Select("DM"), &report.diagnostics[..1], None),
            format!("cannot select DM: {missing_line}")
        );
        assert_eq!(
            error_text(Action::Select("DM"), &report.diagnostics, None),
            format!("cannot select DM: {missing_line} (and 1 more issue)")
        );
    }

    #[test]
    fn member_lines_pad_names_and_statuses() {
        let member = |name: &str, status, size: &str, issues| MemberLine {
            name: name.to_owned(),
            status,
            size: size.to_owned(),
            issues,
        };
        let lines = member_lines(&[
            member("ADSL", Status::Succeeded, "254 rows x 48 columns", 0),
            member("ADAE", Status::Failed, "no output", 1),
            member("ADTTE", Status::Skipped, "no output", 1),
        ]);
        assert_eq!(
            lines,
            [
                "  ADSL   succeeded  254 rows x 48 columns, 0 issues",
                "  ADAE   failed     no output, 1 issue",
                "  ADTTE  skipped    no output, 1 issue",
            ]
        );
    }

    /// A table of these columns in one batch.
    fn table(columns: Vec<(&str, ArrayRef)>) -> Table {
        let schema = Arc::new(Schema::new(
            columns
                .iter()
                .map(|(name, array)| Field::new(*name, array.data_type().clone(), true))
                .collect::<Vec<_>>(),
        ));
        let arrays = columns.into_iter().map(|(_, array)| array).collect();
        let batch = RecordBatch::try_new(schema.clone(), arrays).unwrap();
        Table::from_public(PublicTable::from_batches(schema, vec![batch]).unwrap())
    }

    /// A table of `columns` integer columns named `A`, `B` and so on, each
    /// holding the row numbers from 1, in batches of these sizes.
    fn numbered(columns: u8, batches: &[i64]) -> Table {
        let schema = Arc::new(Schema::new(
            (0..columns)
                .map(|index| {
                    let name = char::from(b'A' + index).to_string();
                    Field::new(name, DataType::Int64, true)
                })
                .collect::<Vec<_>>(),
        ));
        let mut first = 1;
        let batches = batches
            .iter()
            .map(|&rows| {
                let numbers: ArrayRef = Arc::new(Int64Array::from_iter_values(first..first + rows));
                first += rows;
                RecordBatch::try_new(schema.clone(), vec![numbers; usize::from(columns)]).unwrap()
            })
            .collect();
        Table::from_public(PublicTable::from_batches(schema, batches).unwrap())
    }

    /// The text of every value of `array`.
    fn texts(array: &ArrayRef) -> Vec<String> {
        (0..array.len())
            .map(|row| value(array.as_ref(), row).into_owned())
            .collect()
    }

    #[test]
    fn a_cell_prints_at_most_20_characters() {
        assert_eq!(cell(""), "");
        assert_eq!(cell("Hypercholesterolemia"), "Hypercholesterolemia");
        assert_eq!(cell("Hypercholesterolaemia"), "Hypercholesterola...");
        // An escape counts as the characters it prints.
        assert_eq!(cell("Caf\u{e9} au lait"), "Caf\\xe9 au lait");
        assert_eq!(
            cell("M\u{e9}ni\u{e8}re disease of the ear"),
            "M\\xe9ni\\xe8re dis..."
        );
        assert_eq!(cell("line one\nline two\tend"), "line one\\nline tw...");
        // A cut never splits an escape, so a cut cell may print fewer.
        assert_eq!(cell("Insuffisance r\u{e9}nale"), "Insuffisance r...");
        assert_eq!(cell("\u{1f642}\u{1f642}\u{1f642}"), "\\U0001f642...");
        for length in 0..30 {
            for unit in ["a", "\u{e9}", "\u{20ac}", "\u{1f642}"] {
                let text = cell(&unit.repeat(length));
                assert!(text.len() <= CELL_WIDTH && text.is_ascii(), "{text}");
            }
        }
    }

    #[test]
    fn a_column_type_prints_its_language_name() {
        let list = |item| DataType::List(Arc::new(Field::new("item", item, true)));
        for (kind, name) in [
            (DataType::Utf8, "str"),
            (DataType::LargeUtf8, "str"),
            (DataType::Utf8View, "str"),
            (DataType::Int64, "int"),
            (DataType::Float64, "float"),
            (DataType::Date32, "date"),
            (DataType::Timestamp(TimeUnit::Second, None), "datetime"),
            (list(DataType::Utf8), "list<str>"),
            (list(DataType::Int64), "list<int>"),
            (list(list(DataType::Date32)), "list<list<date>>"),
            // Types no engine table has.
            (DataType::Boolean, "?"),
            (DataType::Timestamp(TimeUnit::Millisecond, None), "?"),
            (
                DataType::Timestamp(TimeUnit::Second, Some("UTC".into())),
                "?",
            ),
            (list(DataType::Boolean), "list<?>"),
        ] {
            assert_eq!(type_name(&kind), name, "{kind:?}");
        }
    }

    #[test]
    fn a_value_prints_in_its_canonical_text() {
        let floats: ArrayRef = Arc::new(Float64Array::from(vec![
            Some(1.5),
            Some(180.0),
            Some(-0.0),
            Some(0.1 + 0.2),
            Some(1e21),
            Some(1e-7),
            Some(f64::NAN),
            Some(f64::NEG_INFINITY),
            None,
        ]));
        assert_eq!(
            texts(&floats),
            [
                "1.5",
                "180",
                "-0",
                "0.30000000000000004",
                "1000000000000000000000",
                "0.0000001",
                "null",
                "null",
                "null"
            ]
        );
        let dates: ArrayRef = Arc::new(Date32Array::from(vec![
            Some(0),
            Some(-1),
            Some(-719_162),
            Some(2_932_896),
            Some(2_932_897),
            Some(i32::MIN),
            None,
        ]));
        assert_eq!(
            texts(&dates),
            [
                "1970-01-01",
                "1969-12-31",
                "0001-01-01",
                "9999-12-31",
                "?",
                "?",
                "null"
            ]
        );
        let datetimes: ArrayRef = Arc::new(TimestampSecondArray::from(vec![
            Some(0),
            Some(-1),
            Some(1_700_000_000),
            Some(i64::MAX),
            None,
        ]));
        assert_eq!(
            texts(&datetimes),
            [
                "1970-01-01T00:00:00",
                "1969-12-31T23:59:59",
                "2023-11-14T22:13:20",
                "?",
                "null"
            ]
        );
        let integers: ArrayRef = Arc::new(Int64Array::from(vec![Some(-5), Some(i64::MAX), None]));
        assert_eq!(texts(&integers), ["-5", "9223372036854775807", "null"]);
        // Each string type a host stream may carry.
        let strings: [ArrayRef; 3] = [
            Arc::new(StringArray::from(vec![Some("a b"), Some(""), None])),
            Arc::new(LargeStringArray::from(vec![Some("a b"), Some(""), None])),
            Arc::new(StringViewArray::from(vec![Some("a b"), Some(""), None])),
        ];
        for strings in &strings {
            assert_eq!(texts(strings), ["a b", "", "null"]);
        }
        let flags: ArrayRef = Arc::new(BooleanArray::from(vec![Some(true), None]));
        assert_eq!(texts(&flags), ["?", "null"]);
    }

    #[test]
    fn a_list_prints_its_items_in_brackets() {
        let mut paths = ListBuilder::new(StringBuilder::new());
        paths.values().append_value("columns.AGE");
        paths.values().append_null();
        paths.append(true);
        paths.append(true);
        paths.append(false);
        let paths: ArrayRef = Arc::new(paths.finish());
        assert_eq!(texts(&paths), ["[columns.AGE, null]", "[]", "null"]);
        let mut span = ListBuilder::new(Int64Builder::new());
        span.values().append_value(3);
        span.values().append_value(9);
        span.append(true);
        let span: ArrayRef = Arc::new(span.finish());
        assert_eq!(texts(&span), ["[3, 9]"]);
    }

    /// Thirteen rows in batches of 3, 0, 4 and 6 rows: the first five rows
    /// span two batches, and the last five lie in the last.
    #[test]
    fn a_long_table_prints_its_first_and_last_five_rows_across_batches() {
        assert_eq!(
            preview(&numbered(9, &[3, 0, 4, 6])),
            "\
yamaa.Table: 13 rows x 9 columns
A    B    C    D    E    F    G    H    ...
int  int  int  int  int  int  int  int  ...
1    1    1    1    1    1    1    1    ...
2    2    2    2    2    2    2    2    ...
3    3    3    3    3    3    3    3    ...
4    4    4    4    4    4    4    4    ...
5    5    5    5    5    5    5    5    ...
...
9    9    9    9    9    9    9    9    ...
10   10   10   10   10   10   10   10   ...
11   11   11   11   11   11   11   11   ...
12   12   12   12   12   12   12   12   ...
13   13   13   13   13   13   13   13   ..."
        );
    }

    #[test]
    fn ten_rows_of_eight_columns_print_whole() {
        let printed = preview(&numbered(8, &[4, 6]));
        let lines = printed.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 13, "{printed}");
        assert_eq!(lines[0], "yamaa.Table: 10 rows x 8 columns");
        assert_eq!(lines[1], "A    B    C    D    E    F    G    H");
        assert_eq!(lines[12], "10   10   10   10   10   10   10   10");
        assert!(!printed.contains(ELIDED), "{printed}");
        // Eleven rows leave the sixth out.
        let printed = preview(&numbered(1, &[11]));
        assert_eq!(
            printed,
            "yamaa.Table: 11 rows x 1 column\nA\nint\n1\n2\n3\n4\n5\n...\n7\n8\n9\n10\n11"
        );
    }

    /// Names are cut like any cell, a list prints in brackets, and padding
    /// never ends a line, even after an empty string.
    #[test]
    fn names_are_cells_and_no_line_ends_in_a_space() {
        let mut paths = ListBuilder::new(StringBuilder::new());
        for row in [
            &["columns.AGE"][..],
            &[],
            &["columns.AGEU", "output.columns"],
        ] {
            for path in row {
                paths.values().append_value(path);
            }
            paths.append(true);
        }
        let printed = preview(&table(vec![
            (
                "SUBJECT_IDENTIFIER_FOR_THE_STUDY",
                Arc::new(StringArray::from(vec!["01", "02", "03"])),
            ),
            ("spec_paths", Arc::new(paths.finish())),
            (
                "NOTE",
                Arc::new(StringArray::from(vec!["x", "", "caf\u{e9}"])),
            ),
        ]));
        assert_eq!(
            printed,
            "\
yamaa.Table: 3 rows x 3 columns
SUBJECT_IDENTIFIE...  spec_paths            NOTE
str                   list<str>             str
01                    [columns.AGE]         x
02                    []
03                    [columns.AGEU, ou...  caf\\xe9"
        );
    }

    #[test]
    fn a_table_without_columns_prints_its_header_alone() {
        let schema = Arc::new(Schema::empty());
        let options = RecordBatchOptions::new().with_row_count(Some(3));
        let batch =
            RecordBatch::try_new_with_options(schema.clone(), Vec::new(), &options).unwrap();
        let rows =
            Table::from_public(PublicTable::from_batches(schema.clone(), vec![batch]).unwrap());
        assert_eq!(preview(&rows), "yamaa.Table: 3 rows x 0 columns");
        let empty = Table::from_public(PublicTable::from_batches(schema, Vec::new()).unwrap());
        assert_eq!(preview(&empty), "yamaa.Table: 0 rows x 0 columns");
    }

    #[test]
    fn lazy_objects_print_their_kind_and_path() {
        assert_eq!(
            Summary::spec("sdtm/dm.yaml").to_string(),
            "yamaa spec sdtm/dm.yaml"
        );
        assert_eq!(
            Summary::project("project.yaml").to_string(),
            "yamaa project project.yaml"
        );
        assert_eq!(
            Summary::spec("sdtm/dé.yaml").to_string(),
            "yamaa spec sdtm/d\\xe9.yaml"
        );
    }
}
