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
use crate::table::Table;
use yamaa_adapters::public_table::PublicColumn;

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

/// The size of a run from its output's rows and columns: `<r> rows x <c>
/// columns`, or `no output`.
pub(crate) fn output_size(output: Option<(usize, usize)>) -> String {
    match output {
        Some((rows, columns)) => format!(
            "{} x {}",
            count(rows, "row", "rows"),
            count(columns, "column", "columns")
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
        None => file.into_owned(),
    }
}

/// `file` relative to the current directory when the file lies beneath it,
/// and as it is otherwise. `directory` holds the current directory's
/// spellings ([`current_directory`]); the file and each spelling are
/// absolute, with forward slashes. Resolve only the file's parent for this
/// display projection: a caller can name it through a different directory
/// alias. Append the literal suffix, including a refused path's trailing
/// separator. The leaf is never read or followed; capture keeps its own path.
pub(crate) fn relative<'a>(file: &'a str, directory: &[String]) -> Cow<'a, str> {
    if directory.is_empty() {
        return Cow::Borrowed(file);
    }
    let identity = Path::new(file).parent().and_then(|parent| {
        let suffix = file
            .strip_prefix(parent.to_str()?.trim_end_matches('/'))?
            .strip_prefix('/')?;
        let directory = yamaa_adapters::file_configuration::directory_identity(parent).ok()?;
        Some(format!("{directory}{suffix}"))
    });
    let displayed = identity.as_deref().unwrap_or(file);
    let relative = directory.iter().find_map(|spelling| {
        displayed
            .strip_prefix(spelling.trim_end_matches('/'))?
            .strip_prefix('/')
            .filter(|rest| !rest.is_empty())
    });
    match relative {
        Some(rest) => match file.strip_suffix(rest) {
            Some(prefix) => Cow::Borrowed(&file[prefix.len()..]),
            None => Cow::Owned(rest.to_owned()),
        },
        None => Cow::Borrowed(file),
    }
}

/// The current directory's identity and reported spelling, absolute with
/// forward slashes. The identity matches canonical issue locations and the
/// parent identity used by [`relative`]. The reported spelling also keeps
/// the lexical projection when a file's parent cannot be resolved. Empty
/// when the current directory cannot be read.
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
    let header = format!(
        "yamaa.Table: {}",
        output_size(Some((table.rows(), table.public().width())))
    );
    let table = table.public();
    if table.width() == 0 {
        return header;
    }
    let shown = table.width().min(PREVIEW_COLUMNS);
    let rows = table.rows();
    let (head, tail) = if rows > PREVIEW_ROWS {
        (PREVIEW_ROWS / 2, rows - PREVIEW_ROWS / 2)
    } else {
        (rows, rows)
    };
    // The cells of each line below the header; `None` is the `...` line.
    let mut grid = vec![
        Some(
            (0..shown)
                .map(|column| cell(table.name(column)))
                .collect::<Vec<_>>(),
        ),
        Some(
            (0..shown)
                .map(|column| cell(&table.column(column).type_name()))
                .collect(),
        ),
    ];
    let values = |range: Range<usize>| {
        range.map(|row| {
            Some(
                (0..shown)
                    .map(|column| cell(&value(table.column(column), row)))
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
        Some(cells) => laid_out(cells, &widths, table.width() > shown),
        None => ELIDED.to_owned(),
    }));
    lines.join("\n")
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

/// The text of the value at `row` of `column`: `null` when it is missing, a
/// float in VALUE-8's canonical text (a nonfinite float is missing, as at the
/// scalar boundary), a date or datetime in VALUE-17's, and a list as its
/// items' texts joined by `, ` in brackets. A date or datetime outside the
/// years 1 to 9999, which no engine table holds, prints `?`.
fn value(column: &PublicColumn, row: usize) -> Cow<'_, str> {
    if column.is_missing(row) {
        return Cow::Borrowed("null");
    }
    let text = match column {
        PublicColumn::Str(values) => values.get(row).map(Cow::Borrowed),
        PublicColumn::Int(values) => values.get(row).map(|value| Cow::Owned(value.to_string())),
        PublicColumn::Float(values) => values.get(row).map(|value| {
            yamaa_core::value::FiniteFloat::new(value).map_or(Cow::Borrowed("null"), |value| {
                Cow::Owned(yamaa_core::conversion::float_text(value))
            })
        }),
        PublicColumn::Date(values) => values
            .get(row)
            .and_then(|days| yamaa_adapters::epoch::date_from_days(i64::from(days), Precision::Day))
            .map(|date| Cow::Owned(date.to_string())),
        PublicColumn::DateTime(values) => values
            .get(row)
            .and_then(|seconds| {
                yamaa_adapters::epoch::datetime_from_seconds(seconds, Precision::Second)
            })
            .map(|datetime| Cow::Owned(datetime.to_string())),
        PublicColumn::List(lists) => lists.get(row).map(|items| {
            let texts = items
                .map(|item| value(lists.items(), item))
                .collect::<Vec<_>>();
            Cow::Owned(format!("[{}]", texts.join(", ")))
        }),
    };
    text.unwrap_or(Cow::Borrowed("?"))
}

#[cfg(test)]
mod tests {
    use yamaa_adapters::columns::{Text, Values};
    use yamaa_adapters::public_table::{List, PublicTable};

    use super::*;
    use crate::diagnostic::Position;

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
                std::iter::once(located.clone())
                    .chain(spellings_of.iter().map(|file| {
                        format!("{}/adae.yaml", file.to_str().unwrap().replace('\\', "/"))
                    }))
                    .map(|file| relative(&file, &directory).into_owned())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        #[cfg(unix)]
        std::fs::remove_file(&link).unwrap();
        std::fs::remove_dir_all(&directory).unwrap();
        for (spelling, printed) in spellings_of.iter().zip(printed) {
            assert_eq!(
                printed,
                vec!["adae.yaml"; spellings_of.len() + 1],
                "{}",
                spelling.display()
            );
        }
    }

    /// P-PRINT-1 rule 3: display resolves the parent without following the leaf.
    #[cfg(unix)]
    #[test]
    fn a_parent_alias_projects_its_resolved_directory_without_following_the_leaf() {
        let root = crate::test_scratch::Scratch::new("text-parent-alias");
        let root_path = root.path();
        std::fs::create_dir_all(root_path.join("study/nested")).unwrap();
        std::fs::create_dir_all(root_path.join("outside")).unwrap();
        let study = root_path.join("study");
        let outside = root_path.join("outside");
        let alias = root_path.join("alias");
        let nested_alias = root_path.join("nested-alias");
        std::os::unix::fs::symlink(&study, &alias).unwrap();
        std::os::unix::fs::symlink(study.join("nested"), &nested_alias).unwrap();
        std::os::unix::fs::symlink(&outside, study.join("outside-alias")).unwrap();
        std::os::unix::fs::symlink(outside.join("spec.yaml"), study.join("leaf.yaml")).unwrap();
        let directory = spellings(&study);
        // These leaves need not exist: only the parent is resolved for text.
        let outside_file = study.join("outside-alias/spec.yaml");
        for (file, expected, label) in [
            (alias.join("spec.yaml"), "spec.yaml", "parent alias"),
            (alias.join("spec.yaml/"), "spec.yaml/", "trailing separator"),
            (
                alias.join("missing.yaml/"),
                "missing.yaml/",
                "missing leaf trailing separator",
            ),
            (
                alias.join("leaf.yaml/"),
                "leaf.yaml/",
                "link leaf trailing separator",
            ),
            (
                alias.join("nested/spec.yaml"),
                "nested/spec.yaml",
                "nested path through alias",
            ),
            (
                nested_alias.join("spec.yaml"),
                "nested/spec.yaml",
                "alias targets nested directory",
            ),
            (
                study.join("missing/spec.yaml"),
                "missing/spec.yaml",
                "unresolved parent fallback",
            ),
            (
                study.join("leaf.yaml"),
                "leaf.yaml",
                "leaf link not followed",
            ),
            (
                outside_file.clone(),
                outside_file.to_str().unwrap(),
                "alias outside physical directory",
            ),
        ] {
            assert_eq!(
                relative(file.to_str().unwrap(), &directory),
                expected,
                "{label}"
            );
        }
        assert_eq!(
            (
                std::fs::symlink_metadata(study.join("leaf.yaml"))
                    .unwrap()
                    .file_type()
                    .is_symlink(),
                std::fs::read_link(study.join("leaf.yaml")).unwrap()
            ),
            (true, outside.join("spec.yaml")),
            "display leaves the link and its target unchanged"
        );
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
    fn table(columns: Vec<(&str, PublicColumn)>) -> Table {
        let columns = columns
            .into_iter()
            .map(|(name, column)| (name.to_owned(), column))
            .collect();
        Table::from_public(PublicTable::new(columns).unwrap())
    }

    /// A table of `columns` integer columns named `A`, `B` and so on, each
    /// holding the row numbers from 1, in batches of these sizes.
    fn numbered(columns: u8, batches: &[usize]) -> Table {
        let rows = batches.iter().sum::<usize>() as i64;
        let columns = (0..columns)
            .map(|index| {
                let name = char::from(b'A' + index).to_string();
                (name, PublicColumn::Int((1..=rows).map(Some).collect()))
            })
            .collect();
        Table::from_public(PublicTable::chunked(columns, batches.to_vec()).unwrap())
    }

    /// The text of every value of `column`.
    fn texts(column: &PublicColumn) -> Vec<String> {
        (0..column.len())
            .map(|row| value(column, row).into_owned())
            .collect()
    }

    /// A list column of text items, `None` for a missing item or list.
    fn text_lists(lists: &[Option<&[Option<&str>]>]) -> PublicColumn {
        let mut column = List::new(PublicColumn::Str(Text::default()));
        for list in lists {
            if let (Some(items), PublicColumn::Str(text)) = (list, column.items_mut()) {
                for item in *items {
                    text.push(*item);
                }
            }
            column.close(list.is_some());
        }
        PublicColumn::List(Box::new(column))
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
        let list = |items| PublicColumn::List(Box::new(List::new(items)));
        for (column, name) in [
            (PublicColumn::Str(Text::default()), "str"),
            (PublicColumn::Int(Values::default()), "int"),
            (PublicColumn::Float(Values::default()), "float"),
            (PublicColumn::Date(Values::default()), "date"),
            (PublicColumn::DateTime(Values::default()), "datetime"),
            (list(PublicColumn::Str(Text::default())), "list<str>"),
            (list(PublicColumn::Int(Values::default())), "list<int>"),
            (
                list(list(PublicColumn::Date(Values::default()))),
                "list<list<date>>",
            ),
        ] {
            assert_eq!(column.type_name(), name, "{column:?}");
        }
    }

    #[test]
    fn a_value_prints_in_its_canonical_text() {
        let floats = PublicColumn::Float(
            [
                Some(1.5),
                Some(180.0),
                Some(-0.0),
                Some(0.1 + 0.2),
                Some(1e21),
                Some(1e-7),
                Some(f64::NAN),
                Some(f64::NEG_INFINITY),
                None,
            ]
            .into_iter()
            .collect(),
        );
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
        let dates = PublicColumn::Date(
            [
                Some(0),
                Some(-1),
                Some(-719_162),
                Some(2_932_896),
                Some(2_932_897),
                Some(i32::MIN),
                None,
            ]
            .into_iter()
            .collect(),
        );
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
        let datetimes = PublicColumn::DateTime(
            [Some(0), Some(-1), Some(1_700_000_000), Some(i64::MAX), None]
                .into_iter()
                .collect(),
        );
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
        let integers = PublicColumn::Int([Some(-5), Some(i64::MAX), None].into_iter().collect());
        assert_eq!(texts(&integers), ["-5", "9223372036854775807", "null"]);
        let strings = PublicColumn::Str([Some("a b"), Some(""), None].into_iter().collect());
        assert_eq!(texts(&strings), ["a b", "", "null"]);
    }

    #[test]
    fn a_list_prints_its_items_in_brackets() {
        let paths = text_lists(&[Some(&[Some("columns.AGE"), None]), Some(&[]), None]);
        assert_eq!(texts(&paths), ["[columns.AGE, null]", "[]", "null"]);
        let span = PublicColumn::List(Box::new(List::int([Some(vec![3, 9])])));
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
        let paths = List::text([
            Some(vec!["columns.AGE"]),
            Some(vec![]),
            Some(vec!["columns.AGEU", "output.columns"]),
        ]);
        let text = |values: [&str; 3]| PublicColumn::Str(values.map(Some).into_iter().collect());
        let printed = preview(&table(vec![
            ("SUBJECT_IDENTIFIER_FOR_THE_STUDY", text(["01", "02", "03"])),
            ("spec_paths", PublicColumn::List(Box::new(paths))),
            ("NOTE", text(["x", "", "caf\u{e9}"])),
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
        let rows = Table::from_public(PublicTable::chunked(Vec::new(), vec![3]).unwrap());
        assert_eq!(preview(&rows), "yamaa.Table: 3 rows x 0 columns");
        let empty = Table::from_public(PublicTable::chunked(Vec::new(), Vec::new()).unwrap());
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
