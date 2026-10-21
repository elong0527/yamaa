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
//!
//! Every line is ASCII and holds no line break (rule 5). A character outside
//! printable ASCII prints as the escape the YAML reader writes: `\t`, `\n`
//! and `\r`, then `\xHH`, `\uHHHH` or `\UHHHHHHHH` by code point, in lowercase
//! hexadecimal; a backslash prints as itself. Counts are plain integers, with
//! the singular noun for 1, and nothing depends on a terminal.

use std::fmt::{self, Write as _};

use crate::api::Status;
use crate::diagnostic::{Diagnostic, Location};
use crate::table::Table;

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
            let _ = write!(text, ": {}", issue_line(first, directory.as_deref()));
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
            .map(|issue| format!("first issue: {}", issue_line(issue, directory.as_deref()))),
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

/// One issue's printed line (rule 3), the location relative to `directory`.
pub(crate) fn issue_line(diagnostic: &Diagnostic, directory: Option<&str>) -> String {
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
fn located(location: &Location, directory: Option<&str>) -> String {
    let file = relative(&location.file, directory);
    match location.position {
        Some(position) => format!("{file}:{}:{}", position.line, position.column),
        None => file.to_owned(),
    }
}

/// `file` relative to `directory` when the file lies beneath it, and as it
/// is otherwise. Both are absolute, with forward slashes.
pub(crate) fn relative<'a>(file: &'a str, directory: Option<&str>) -> &'a str {
    directory
        .and_then(|directory| file.strip_prefix(directory.trim_end_matches('/')))
        .and_then(|rest| rest.strip_prefix('/'))
        .filter(|rest| !rest.is_empty())
        .unwrap_or(file)
}

/// The current directory, absolute with forward slashes, when it can be
/// read as text.
pub(crate) fn current_directory() -> Option<String> {
    let directory = std::env::current_dir().ok()?;
    Some(directory.to_str()?.replace('\\', "/"))
}

/// `n` and the noun, singular for 1.
pub(crate) fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `text` with every character outside printable ASCII escaped (rule 5).
pub(crate) fn ascii(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
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
    out
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn a_file_beneath_the_directory_prints_relative_to_it() {
        let directory = Some("/study");
        assert_eq!(
            relative("/study/adam/adae.yaml", directory),
            "adam/adae.yaml"
        );
        assert_eq!(
            relative("/study/adam/adae.yaml", Some("/study/")),
            "adam/adae.yaml"
        );
        assert_eq!(
            relative("/studyx/adae.yaml", directory),
            "/studyx/adae.yaml"
        );
        assert_eq!(relative("/other/adae.yaml", directory), "/other/adae.yaml");
        assert_eq!(relative("/study", directory), "/study");
        assert_eq!(relative("/study/adae.yaml", None), "/study/adae.yaml");
        assert_eq!(relative("C:/study/dm.yaml", Some("C:/study")), "dm.yaml");
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
        assert_eq!(located(&file(at), Some("/study")), "adam/adae.yaml:57:9");
        assert_eq!(located(&file(None), Some("/study")), "adam/adae.yaml");
        assert_eq!(
            located(&file(at), Some("/elsewhere")),
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
        let missing_line = issue_line(missing, None);
        assert!(missing_line.ends_with(&format!(
            " [{}, {}]",
            missing.code,
            missing.rule.as_deref().unwrap()
        )));
        // The message's non-ASCII value prints escaped.
        assert!(missing_line.contains("in/\\xe9.csv"), "{missing_line}");
        assert!(issue_line(rejected, None).ends_with(&format!(" [{}]", rejected.code)));
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
