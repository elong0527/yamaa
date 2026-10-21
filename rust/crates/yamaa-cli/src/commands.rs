//! The commands that run on the `yamaa` facade (#3030 section 2.8, D12):
//! each calls the facade as a host does and prints the facade's own words
//! (D37, D39, D103), so the command line and the hosts say the same thing.
//!
//! - **Text.** A result prints its summary line, a project run's member
//!   lines and every issue's line, then the files its save wrote, one
//!   absolute path per line in publication order (D38, D99); a refused save
//!   prints the files it wrote before it stopped, such as a failed run's
//!   report (PROJECT-12). A refusal of the call itself prints D39's error
//!   text in place of the summary line: it carries the first issue's line, so
//!   the lines of the other issues follow. `lint` prints each finding's line,
//!   then its summary line; `resolve` prints the resolved specification as
//!   canonical JSON.
//! - **JSON** (`--format json`). One canonical JSON document (RFC 8785) on
//!   one line, with the keys the draft `rust/interface/bindings-1.0.yaml`
//!   lists in `cli.commands[].json_keys`: the command and its arguments, the
//!   `status`, the issues as objects whose `context` is an object, and the
//!   written `paths`. `resolve` prints the resolved specification itself.
//!   `submission` leaves out `created`: the facade does not give the
//!   creation time a submission used yet.
//! - **Exit status.** 0 when the result succeeded and its save wrote
//!   everything, when `resolve` printed, and when no lint finding remains; 1
//!   for a `failed` or `skipped` result, a refusal or a remaining lint
//!   finding (D68, D98). An argument the facade cannot take, a value that is
//!   not UTF-8 text, exits 2.
//!
//! Each command dispatches on its PATH's `type:` header, which the facade
//! does not read yet. A PATH given `--spec` or `--section`, or given to
//! `submission`, can only name a project, so those calls run; a PATH without
//! them is refused as not implemented yet (D96), and so are `read-odm`,
//! `schema` and `selftest`.

use std::collections::BTreeMap;
use std::ffi::OsStr;

use yamaa::{
    Action, CancellationToken, ContextValue, Diagnostic, Engine, EngineOptions, Failure, Host,
    LintFinding, ProjectRun, Status, Summary,
};

use crate::arguments::{Call, Format, Usage};
use crate::{FINDINGS, SUCCEEDED};

/// What became of a command.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Execution {
    /// The command ran: what it prints on standard output and its exit
    /// status, 0 or 1.
    Ran { stdout: String, status: u8 },
    /// An argument the facade cannot take: exit status 2.
    Refused(Usage),
    /// A command, or a form of one, not implemented yet (D96): printed as
    /// `yamaa: <what> is not implemented yet`, with exit status 2.
    NotImplemented(String),
}

/// Run `call` and render what it prints in `format`.
pub(crate) fn execute(call: &Call, format: Format) -> Execution {
    let printed = match call.name {
        "check" => check(call),
        "run" => run(call),
        "submission" => submission(call),
        "resolve" => resolve(call),
        "lint" => lint(call),
        // `read-odm` needs a writer for OUT and `schema` the schema text,
        // which the facade does not offer yet; `selftest` waits for #3062.
        other => Err(Execution::NotImplemented(other.to_owned())),
    };
    match printed {
        Ok(printed) => printed.render(format),
        Err(execution) => execution,
    }
}

/// One JSON document's members, in key order.
type Document = BTreeMap<String, ContextValue>;

/// What one command prints, in both formats, and its exit status. The text
/// lines and the JSON document are built together, so they say the same
/// thing.
struct Printed {
    lines: Vec<String>,
    document: ContextValue,
    status: u8,
}

impl Printed {
    fn render(self, format: Format) -> Execution {
        let stdout = match format {
            Format::Text => self.lines.iter().map(|line| format!("{line}\n")).collect(),
            Format::Json => format!("{}\n", self.document.to_json()),
        };
        Execution::Ran {
            stdout,
            status: self.status,
        }
    }
}

fn text(value: &str) -> ContextValue {
    ContextValue::Text(value.to_owned())
}

fn count(value: usize) -> ContextValue {
    ContextValue::Int(i64::try_from(value).unwrap_or(i64::MAX))
}

fn texts(values: &[String]) -> ContextValue {
    ContextValue::List(values.iter().map(|value| text(value)).collect())
}

fn issues(diagnostics: &[Diagnostic]) -> ContextValue {
    ContextValue::List(diagnostics.iter().map(Diagnostic::to_value).collect())
}

/// The document of `command`, with the arguments it names as `(key, value)`.
fn document<const N: usize>(command: &str, arguments: [(&str, ContextValue); N]) -> Document {
    std::iter::once(("command", text(command)))
        .chain(arguments)
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

/// The exit status of a result whose status is `status`.
fn exit(status: Status) -> u8 {
    if status == Status::Succeeded {
        SUCCEEDED
    } else {
        FINDINGS
    }
}

/// A result's printed form: its summary line and member lines, every issue,
/// then, after a save, the files it wrote. `issues` are the result's issues
/// read after its save, which a save's own failure joins. A refused save's
/// issues that the result does not hold, such as `nothing_to_save`, print
/// after the result's own, and the save's paths are those written before it
/// stopped. The status describes the operation, not the save, so a refused
/// save of a result that succeeded keeps `succeeded` and exits 1.
fn result(
    mut document: Document,
    summary: &Summary,
    status: Status,
    mut issues: Vec<Diagnostic>,
    saved: Option<Result<Vec<String>, Failure>>,
) -> Printed {
    let mut refused = false;
    let paths = saved.map(|saved| match saved {
        Ok(paths) => paths,
        Err(failure) => {
            refused = true;
            for issue in failure.diagnostics {
                if !issues.contains(&issue) {
                    issues.push(issue);
                }
            }
            failure.paths
        }
    });
    let mut lines = vec![summary.line.clone()];
    lines.extend(summary.members.iter().cloned());
    lines.extend(issues.iter().map(Diagnostic::text));
    document.insert("status".into(), text(status.as_str()));
    document.insert("issues".into(), self::issues(&issues));
    if let Some(paths) = paths {
        document.insert("paths".into(), texts(&paths));
        lines.extend(paths);
    }
    Printed {
        lines,
        document: ContextValue::Map(document),
        status: if refused { FINDINGS } else { exit(status) },
    }
}

/// A refusal's printed form: D39's error text, which carries the first
/// issue's line and counts the others, then each other issue's line, so
/// every issue prints once, then the files written before it. Its JSON is
/// the command's document with the status `failed`, every issue and, for a
/// command that writes, the paths.
fn refusal(mut document: Document, failure: Failure) -> Printed {
    let mut lines = vec![failure.message];
    lines.extend(failure.diagnostics.iter().skip(1).map(Diagnostic::text));
    document.insert("status".into(), text(Status::Failed.as_str()));
    document.insert("issues".into(), issues(&failure.diagnostics));
    if document.contains_key("paths") {
        document.insert("paths".into(), texts(&failure.paths));
    }
    lines.extend(failure.paths);
    Printed {
        lines,
        document: ContextValue::Map(document),
        status: FINDINGS,
    }
}

/// `value` as text: the facade takes paths, names and times as UTF-8 text,
/// so a value that is not is an argument it cannot take.
fn utf8<'a>(call: &Call, what: &str, value: &'a OsStr) -> Result<&'a str, Execution> {
    value.to_str().ok_or_else(|| {
        Execution::Refused(Usage(format!(
            "{}: {what} is not UTF-8: {}",
            call.name,
            value.to_string_lossy()
        )))
    })
}

/// The command's first positional argument as text.
fn path(call: &Call) -> Result<&str, Execution> {
    utf8(call, "PATH", &call.arguments[0])
}

/// The value given to `flag` as text, when the flag was given.
fn flag<'a>(call: &'a Call, flag: &str) -> Result<Option<&'a str>, Execution> {
    call.value(flag)
        .map(|value| utf8(call, flag, value))
        .transpose()
}

fn not_implemented(what: &str) -> Execution {
    Execution::NotImplemented(what.to_owned())
}

/// The command line's engine: the local file resolver, and no host language
/// (#3030 section 2.10, D123). It reads no project's `environments.<host>`
/// entry, so it verifies no lock and activates nothing: a build that calls a
/// project function is refused before it reads data, and every other build
/// runs as a host runs it (section 2.3, D110).
fn engine() -> Engine {
    Engine::new(EngineOptions::neutral(
        Host::Cli.as_str().to_owned(),
        String::new(),
    ))
}

/// `yamaa check PROJECT --spec NAME`: `p.spec(name).check()`.
fn check(call: &Call) -> Result<Printed, Execution> {
    let Some(name) = flag(call, "--spec")? else {
        return Err(not_implemented("check without --spec"));
    };
    let path = path(call)?;
    let document = document("check", [("path", text(path)), ("spec", text(name))]);
    let engine = engine();
    let spec = match engine.project_spec(path, name) {
        Ok(spec) => spec,
        Err(failure) => return Ok(refusal(document, failure)),
    };
    Ok(
        match engine.spec_check(&spec, Some(path), &CancellationToken::new()) {
            Ok(check) => result(
                document,
                &check.summary(),
                check.status(),
                check.diagnostics(),
                None,
            ),
            Err(error) => refusal(document, Failure::from_error(Action::Read(&spec), &error)),
        },
    )
}

/// What `run` builds: one specification a project lists, or the scope of one
/// section.
enum Scope<'a> {
    Spec(&'a str),
    Section(&'a str),
}

/// `yamaa run PROJECT --spec NAME`, `p.spec(name).run()`, or `yamaa run
/// PROJECT --section SECTION`, `p.run(section)`; then `save()`.
fn run(call: &Call) -> Result<Printed, Execution> {
    let name = flag(call, "--spec")?;
    let section = flag(call, "--section")?;
    // The arguments give one of the two at most.
    let scope = match (name, section) {
        (Some(name), _) => Scope::Spec(name),
        (None, Some(section)) => Scope::Section(section),
        (None, None) => return Err(not_implemented("run without --spec or --section")),
    };
    let path = path(call)?;
    let optional = |value: Option<&str>| value.map_or(ContextValue::Null, text);
    let mut document = document(
        "run",
        [
            ("path", text(path)),
            ("spec", optional(name)),
            ("section", optional(section)),
            ("members", ContextValue::List(Vec::new())),
            ("paths", ContextValue::List(Vec::new())),
        ],
    );
    let engine = engine();
    let token = CancellationToken::new();
    Ok(match scope {
        Scope::Spec(name) => {
            let spec = match engine.project_spec(path, name) {
                Ok(spec) => spec,
                Err(failure) => return Ok(refusal(document, failure)),
            };
            match engine.spec_run(&spec, Some(path), &token) {
                Ok(mut run) => {
                    let saved = run.save();
                    result(
                        document,
                        &run.summary(),
                        run.status(),
                        run.diagnostics(),
                        Some(saved),
                    )
                }
                Err(error) => refusal(document, Failure::from_error(Action::Read(&spec), &error)),
            }
        }
        // An unknown section refuses the run (D98).
        Scope::Section(section) => match engine.project_run(path, Some(section), &token) {
            Ok(mut runs) => {
                let saved = runs.save();
                document.insert("members".into(), members(&runs));
                result(
                    document,
                    &runs.summary(),
                    runs.status(),
                    runs.diagnostics(),
                    Some(saved),
                )
            }
            Err(error) => refusal(
                document,
                Failure::from_error(Action::Select(section), &error),
            ),
        },
    })
}

/// A project run's members in run order, as `cli.json.member_keys` gives
/// them. The facade reaches a member only by its name, so a listing that
/// could not be read, which has none, is missing here though its member line
/// prints.
fn members(runs: &ProjectRun) -> ContextValue {
    ContextValue::List(
        runs.names()
            .iter()
            .filter_map(|name| {
                let run = runs.get(name)?;
                let output = run.output();
                let size = |measure: fn(&yamaa::Table) -> usize| {
                    output.map_or(ContextValue::Null, |table| count(measure(table)))
                };
                let values = [
                    ("name", text(name)),
                    ("status", text(run.status().as_str())),
                    ("rows", size(yamaa::Table::rows)),
                    ("columns", size(|table| table.columns().len())),
                    ("issue_count", count(run.diagnostics().len())),
                ];
                Some(ContextValue::Map(
                    values
                        .into_iter()
                        .map(|(key, value)| (key.to_owned(), value))
                        .collect(),
                ))
            })
            .collect(),
    )
}

/// `yamaa submission PROJECT [--section SECTION] [--created TIME]`:
/// `p.submission(section, created)`, then `save()`. Without `--created`,
/// the time used is not printed yet; see the module documentation.
fn submission(call: &Call) -> Result<Printed, Execution> {
    let path = path(call)?;
    let section = flag(call, "--section")?;
    let created = flag(call, "--created")?;
    let document = document(
        "submission",
        [
            ("path", text(path)),
            ("section", section.map_or(ContextValue::Null, text)),
            ("paths", ContextValue::List(Vec::new())),
        ],
    );
    let composed = engine().project_submission(
        path,
        section,
        created,
        yamaa::VERSION,
        &CancellationToken::new(),
    );
    Ok(match composed {
        Ok(mut submission) => {
            let saved = submission.save();
            result(
                document,
                &submission.summary(),
                submission.status(),
                submission.diagnostics(),
                Some(saved),
            )
        }
        // An unknown section refuses the submission (D98).
        Err(error) => {
            let action = section.map_or(Action::Read(path), Action::Select);
            refusal(document, Failure::from_error(action, &error))
        }
    })
}

/// `yamaa resolve PROJECT --spec NAME`: `p.spec(name).resolve()`, printed as
/// canonical JSON in both formats.
fn resolve(call: &Call) -> Result<Printed, Execution> {
    let Some(name) = flag(call, "--spec")? else {
        return Err(not_implemented("resolve without --spec"));
    };
    let path = path(call)?;
    let document = document("resolve", [("path", text(path)), ("spec", text(name))]);
    let engine = engine();
    let resolved = engine
        .project_spec(path, name)
        .and_then(|spec| engine.spec_resolve(&spec, Some(path)));
    Ok(match resolved {
        Ok(value) => Printed {
            lines: vec![value.to_json()],
            document: value,
            status: SUCCEEDED,
        },
        Err(failure) => refusal(document, failure),
    })
}

/// `yamaa lint [--fix] PATH...`: `lint(paths, fix)`. A refusal adds the
/// issues to the lint document's keys.
fn lint(call: &Call) -> Result<Printed, Execution> {
    let paths = call
        .arguments
        .iter()
        .map(|path| utf8(call, "PATH", path).map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    let mut document = document(
        "lint",
        [
            ("findings", ContextValue::List(Vec::new())),
            ("paths", ContextValue::List(Vec::new())),
        ],
    );
    Ok(match yamaa::lint(&paths, call.has("--fix")) {
        Ok(lint) => {
            let findings = lint.findings();
            let status = if findings.is_empty() {
                Status::Succeeded
            } else {
                Status::Failed
            };
            let mut lines = findings.iter().map(LintFinding::text).collect::<Vec<_>>();
            lines.push(lint.summary());
            document.insert("status".into(), text(status.as_str()));
            document.insert(
                "findings".into(),
                ContextValue::List(findings.iter().map(LintFinding::to_value).collect()),
            );
            document.insert("paths".into(), texts(lint.rewritten()));
            Printed {
                lines,
                document: ContextValue::Map(document),
                status: exit(status),
            }
        }
        Err(failure) => refusal(document, failure),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A summary of `line` alone, with no member lines.
    fn summary(line: &str) -> Summary {
        let mut summary = Summary::spec("unused");
        summary.line = line.to_owned();
        summary
    }

    fn rendered(printed: Printed, format: Format) -> (String, u8) {
        match printed.render(format) {
            Execution::Ran { stdout, status } => (stdout, status),
            other => panic!("not run: {other:?}"),
        }
    }

    #[test]
    fn a_result_prints_its_lines_then_its_paths_and_exits_by_its_status() {
        let summary = summary("yamaa run of DM: succeeded, 2 rows x 2 columns, 0 issues");
        let saved = Ok(vec!["/s/dm-report.csv".to_owned(), "/s/dm.csv".to_owned()]);
        let printed = |status, saved| {
            result(
                document("run", [("path", text("p.yaml"))]),
                &summary,
                status,
                Vec::new(),
                saved,
            )
        };
        assert_eq!(
            rendered(printed(Status::Succeeded, Some(saved.clone())), Format::Text),
            (
                "yamaa run of DM: succeeded, 2 rows x 2 columns, 0 issues\n/s/dm-report.csv\n/s/dm.csv\n"
                    .to_owned(),
                0
            )
        );
        assert_eq!(
            rendered(printed(Status::Succeeded, Some(saved)), Format::Json),
            (
                "{\"command\":\"run\",\"issues\":[],\"path\":\"p.yaml\",\"paths\":[\"/s/dm-report.csv\",\"/s/dm.csv\"],\"status\":\"succeeded\"}\n"
                    .to_owned(),
                0
            )
        );
        // A check saves nothing, so it has no paths.
        assert_eq!(
            rendered(printed(Status::Skipped, None), Format::Json),
            (
                "{\"command\":\"run\",\"issues\":[],\"path\":\"p.yaml\",\"status\":\"skipped\"}\n"
                    .to_owned(),
                1
            )
        );
    }

    #[test]
    fn a_refusal_prints_its_error_text_and_has_the_status_failed() {
        let failure = Failure::from_error(Action::Read("odm.xml"), &yamaa::Error::Cancelled);
        let printed = refusal(
            document("run", [("paths", ContextValue::List(Vec::new()))]),
            failure.clone(),
        );
        assert_eq!(
            rendered(printed, Format::Text),
            ("cannot read odm.xml: operation cancelled\n".to_owned(), 1)
        );
        let printed = refusal(document("check", []), failure);
        assert_eq!(
            rendered(printed, Format::Json),
            (
                "{\"command\":\"check\",\"issues\":[],\"status\":\"failed\"}\n".to_owned(),
                1
            )
        );
    }
}
