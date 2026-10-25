//! The commands that run on the `yamaa` facade (#3030 section 2.8, D12):
//! each calls the facade as a host does and prints the facade's own words
//! (D37, D39, D103), so the command line and the hosts say the same thing.
//!
//! - **Text.** A result prints its summary line, a project run's member
//!   lines and every issue's line, then the files its save wrote, one
//!   absolute path per line in publication order (D38, D99); a refused save
//!   prints the files it wrote before it stopped, such as a failed run's
//!   report (PROJECT-12). A submission made without `--created` prints the
//!   time it read for every section, `created: <time>`, after its summary
//!   line (D129). A refusal of the call itself prints D39's error text in
//!   place of the summary line: it carries the first issue's line, so the
//!   lines of the other issues follow. `lint` prints each finding's line,
//!   then its summary line; `resolve` prints the resolved specification as
//!   canonical JSON, and `schema` the JSON Schema of a file type as the
//!   facade gives it; `read-odm` prints only the file it saved, as an
//!   absolute path, or, when the save is refused, D39's error text and the
//!   lines of the other issues, as a refusal does. `conditions` prints the
//!   condition catalogue as a table (#3700): a line of its column names, then
//!   one line per condition, each cell separated by a tab, a list's items
//!   joined with `, ` and a missing cell empty. `selftest` prints the
//!   facade's report as the hosts print it (D119): its summary line, `first
//!   failure:` when a case failed, `not covered:` when an adapter is not
//!   covered, then the engine, language, host and platform.
//! - **JSON** (`--format json`). One canonical JSON document (RFC 8785) on
//!   one line, with the keys `rust/interface/bindings.yaml` lists in
//!   `cli.commands[].json_keys`, or in `refusal_keys` where a refusal's
//!   differ: the command and its arguments, the `status`, the issues as
//!   objects whose `context` is an object, and the written `paths`. `resolve` prints the resolved specification itself,
//!   and `schema` the JSON Schema itself (D149); a refusal of either prints
//!   the command's document, which echoes its arguments: `resolve`'s `path`
//!   and `spec`, and `schema`'s `type` (D169). A submission's `created` is
//!   the time its documents carry (D129); a refused submission has none, so
//!   its `created` is `--created` as given, or null. `read-odm` names SOURCE
//!   as its `path` and counts the items read in `rows`. Its `status` follows
//!   the read, as a run's follows the run: a read that completed is
//!   `succeeded` even when the save of OUT is then refused, which shows as
//!   the save's issues and exit status 1, and only a refused read is `failed`
//!   (D169). `conditions` gives each condition as an object with the
//!   catalogue's columns, its lists as lists. `selftest` gives its `cases`
//!   and the engine `info` as `--version` prints it; the adapters no case
//!   covers are named on its printed line alone (D119).
//! - **Exit status.** 0 when the result succeeded and its save wrote
//!   everything, when `read-odm` saved OUT, when `resolve`, `schema` or
//!   `conditions` printed, when no lint finding remains, and when no
//!   selftest case failed; 1 for a `failed` or `skipped` result, a refusal,
//!   a remaining lint finding or a failed selftest case (D68, D98). A
//!   selftest case that this build does not cover fails nothing (D119). An
//!   argument the facade cannot take, a value that is not UTF-8 text, exits
//!   2.
//!
//! `check`, `run`, `submission` and `resolve` dispatch on the type their PATH
//! declares in its `type:` header, which the facade reads before anything
//! else reads the file (D122). A spec is checked, run or resolved alone, or
//! checked through `--parent` as a fragment on the spec that flag names
//! (#3694); a project is checked or run whole, or through `--spec NAME` one
//! specification it lists, or through `--section` one section, and
//! `submission` composes it.
//! A file the facade cannot type is a file problem it explains with issues,
//! so the call is refused with exit status 1 (D68, D125): a path it cannot
//! read, a file written in language version 0.5, which declares
//! `schema_version`, or a `type` naming no file type. A PATH whose type the
//! command does not take, at all or with the flags given, such as `--spec`
//! with a spec or `--parent` with a project, is a usage error, which exits 2.
//! `selftest` takes no PATH: it runs the cases the facade carries on an
//! engine like the command line's, which claims no host language (D108,
//! D123). `read-odm SOURCE OUT` reads SOURCE as the hosts' `read_odm()` reads
//! it, then saves the item table to OUT, whose extension selects CSV or
//! Parquet (D134). The facade decides both: a SOURCE it cannot read, a bound
//! below its minimum, and an OUT with another extension, which it refuses
//! before writing anything, are refusals with exit status 1. `schema TYPE`
//! passes TYPE to the facade as given, so a TYPE that names no file type is
//! the facade's refusal, `unexpected_file_type`, with exit status 1, as an
//! unknown section is (D98, D149).

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::num::NonZeroUsize;

use yamaa::{
    CancellationToken, Condition, ContextValue, Diagnostic, Engine, EngineOptions, Failure,
    FileType, Host, Info, LintFinding, OdmOptions, Partitions, ProjectRequest, ProjectRun, Run,
    Status, SubmissionOptions, Summary,
};

use crate::arguments::{Call, Format, Usage};
use crate::{FINDINGS, SUCCEEDED};

/// What became of a command.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Execution {
    /// The command ran: what it prints on standard output and its exit
    /// status, 0 or 1.
    Ran { stdout: String, status: u8 },
    /// An argument the facade cannot take, or a PATH whose type the command
    /// does not take: exit status 2.
    Refused(Usage),
}

/// Run `call` and render what it prints in `format`.
pub(crate) fn execute(call: &Call, format: Format, token: &CancellationToken) -> Execution {
    let printed = match call.name {
        "check" => check(call, token),
        "run" => run(call, token),
        "submission" => submission(call, token),
        "resolve" => resolve(call),
        "read-odm" => read_odm(call),
        "lint" => lint(call),
        "schema" => schema(call),
        "conditions" => Ok(conditions()),
        "selftest" => Ok(selftest()),
        other => unreachable!("the arguments admit no command {other}"),
    };
    match printed {
        Ok(printed) => printed.render(format),
        Err(execution) => execution,
    }
}

/// One JSON document's members, in key order.
type Document = BTreeMap<String, ContextValue>;

/// What one command prints, in both formats, and its exit status. The text
/// lines and the JSON text are built together, so they say the same thing.
struct Printed {
    lines: Vec<String>,
    /// The canonical JSON text `--format json` prints, without its line
    /// feed: a document the command builds, or JSON text the facade gives
    /// as it is.
    json: String,
    status: u8,
}

impl Printed {
    fn render(self, format: Format) -> Execution {
        let stdout = match format {
            Format::Text => self.lines.iter().map(|line| format!("{line}\n")).collect(),
            Format::Json => format!("{}\n", self.json),
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

/// A flag's value as text, or null when the flag was not given.
fn optional(value: Option<&str>) -> ContextValue {
    value.map_or(ContextValue::Null, text)
}

fn issues(diagnostics: &[Diagnostic]) -> ContextValue {
    ContextValue::List(diagnostics.iter().map(Diagnostic::to_value).collect())
}

/// One JSON object of `(key, value)` members.
fn object<const N: usize>(members: [(&str, ContextValue); N]) -> ContextValue {
    ContextValue::Map(
        members
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

/// The engine information as one object with `cli.json.info_keys`, as
/// `--version` prints it and `selftest` reports it. The command line runs on
/// no interpreter, so its `host_version` is null.
pub(crate) fn info(info: &Info) -> ContextValue {
    object([
        ("version", text(info.version)),
        ("language_versions", texts(&info.language_versions)),
        (
            "features",
            ContextValue::List(info.features.iter().map(|feature| text(feature)).collect()),
        ),
        ("host", text(info.host.as_str())),
        ("host_version", optional(info.host_version.as_deref())),
        ("platform", text(&info.platform)),
    ])
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

/// The lines of a result's summary before its first issue's: the summary
/// line and any member lines, which the command line prints before every
/// issue's line. `issues` are the result's own, read with its summary, which
/// ends with the first issue's line exactly when they are not empty.
fn heading(summary: &Summary, issues: &[Diagnostic]) -> Vec<String> {
    let mut lines = summary.lines().map(str::to_owned).collect::<Vec<_>>();
    if !issues.is_empty() {
        lines.pop();
    }
    lines
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
    let mut lines = heading(summary, &issues);
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
    lines.extend(issues.iter().map(Diagnostic::text));
    document.insert("status".into(), text(status.as_str()));
    document.insert("issues".into(), self::issues(&issues));
    if let Some(paths) = paths {
        document.insert("paths".into(), texts(&paths));
        lines.extend(paths);
    }
    Printed {
        lines,
        json: ContextValue::Map(document).to_json(),
        status: if refused { FINDINGS } else { exit(status) },
    }
}

/// A refusal's printed form: D39's error text, which carries the first
/// issue's line and counts the others, then each other issue's line, so
/// every issue prints once, then the files written before it. Its JSON is
/// the command's document with the status `failed`, every issue and, for a
/// command that writes, the paths.
fn refusal(document: Document, failure: Failure) -> Printed {
    refusal_with(document, Status::Failed, failure)
}

/// A refusal printed as [`refusal`] prints it, with exit status 1, whose
/// JSON `status` is `status`: `failed` when the call itself is refused, or
/// the status of the result whose save was refused, since `status`
/// describes the result, not the save (D36, D169).
fn refusal_with(mut document: Document, status: Status, failure: Failure) -> Printed {
    let mut lines = vec![failure.message];
    lines.extend(failure.diagnostics.iter().skip(1).map(Diagnostic::text));
    document.insert("status".into(), text(status.as_str()));
    document.insert("issues".into(), issues(&failure.diagnostics));
    if document.contains_key("paths") {
        document.insert("paths".into(), texts(&failure.paths));
    }
    lines.extend(failure.paths);
    Printed {
        lines,
        json: ContextValue::Map(document).to_json(),
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

/// A PATH whose declared type the command does not take, at all or in the
/// form `how` names, such as `with --spec`: a usage error, which exits 2
/// (D68).
fn not_taken(call: &Call, path: &str, file_type: FileType, how: Option<&str>) -> Execution {
    let how = how.map_or_else(String::new, |how| format!(" {how}"));
    Execution::Refused(Usage(format!(
        "{name}: {path} declares type {}, which {name} does not take{how}",
        file_type.as_str(),
        name = call.name,
    )))
}

/// The partitions every build runs over (#3794, D264): key-bounded over the
/// specification's two leading declared keys, of at most 624,640 driver
/// records each, with the whole-table fallback. The command line depends on
/// the facade alone, so it spells core's `Limits::PARTITION_KEYS` and
/// `Limits::PARTITION_ROWS`, which the facade's `client_partitions.rs` checks.
const PARTITIONS: [usize; 2] = [2, 624_640];

/// The command line's engine: the local file resolver, no host language
/// (#3030 section 2.10, D123) and [`PARTITIONS`]. It reads no project's
/// `environments.<host>` entry, so it verifies no lock and activates
/// nothing: a build that calls a project function is refused before it
/// reads data, and every other build runs as a host runs it (section 2.3,
/// D110).
fn engine() -> Engine {
    let [keys, rows] = PARTITIONS.map(|count| NonZeroUsize::new(count).expect("nonzero"));
    Engine::new(
        EngineOptions::neutral(Host::Cli.as_str().to_owned(), String::new())
            .with_partitions(Partitions::by_keys(keys, rows).with_whole_table_fallback()),
    )
}

/// The request of the project `path`, polling `token`, whose run and
/// submission take `section` when one is given.
fn project_request<'a>(
    engine: &'a Engine,
    path: &'a str,
    section: Option<&'a str>,
    token: &'a CancellationToken,
) -> ProjectRequest<'a> {
    let request = engine.project(path).cancel_with(token);
    match section {
        Some(section) => request.section(section),
        None => request,
    }
}

/// `yamaa check PATH`: `s.check()` of a spec, `p.check()` of a project, or,
/// with `--spec NAME`, `p.spec(name).check()`, or, with `--parent`,
/// `s.check(parent)` of a spec, a fragment checked on the spec that flag
/// names as its parent (#3694). The arguments give `--spec` or `--parent`,
/// not both.
fn check(call: &Call, token: &CancellationToken) -> Result<Printed, Execution> {
    let path = path(call)?;
    let name = flag(call, "--spec")?;
    let parent = flag(call, "--parent")?;
    let document = document("check", [("path", text(path)), ("spec", optional(name))]);
    let engine = engine();
    let file_type = match engine.file_type(path) {
        Ok(file_type) => file_type,
        Err(failure) => return Ok(refusal(document, failure)),
    };
    let checked = match (file_type, name, parent) {
        (FileType::Spec, None, None) => engine.spec(path).cancel_with(token).check(),
        (FileType::Spec, None, Some(parent)) => {
            engine.spec(path).cancel_with(token).check_on(parent)
        }
        (FileType::Project, None, None) => engine.project(path).cancel_with(token).check(),
        (FileType::Project, Some(name), None) => {
            match engine.project(path).cancel_with(token).spec(name) {
                Ok(spec) => spec.check(),
                Err(failure) => return Ok(refusal(document, failure)),
            }
        }
        (FileType::Spec, Some(_), _) => {
            return Err(not_taken(call, path, file_type, Some("with --spec")))
        }
        (FileType::Project, _, Some(_)) => {
            return Err(not_taken(call, path, file_type, Some("with --parent")))
        }
        (other, ..) => return Err(not_taken(call, path, other, None)),
    };
    Ok(match checked {
        Ok(check) => result(
            document,
            &check.summary(),
            check.status(),
            check.diagnostics(),
            None,
        ),
        Err(failure) => refusal(document, failure),
    })
}

/// `yamaa run PATH`: `s.run()` of a spec, `p.run()` of a project, or, with
/// `--spec NAME` or `--section SECTION`, `p.spec(name).run()` or
/// `p.run(section)`; then `save()`.
fn run(call: &Call, token: &CancellationToken) -> Result<Printed, Execution> {
    let path = path(call)?;
    let name = flag(call, "--spec")?;
    let section = flag(call, "--section")?;
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
    let file_type = match engine.file_type(path) {
        Ok(file_type) => file_type,
        Err(failure) => return Ok(refusal(document, failure)),
    };
    // The arguments give `--spec` or `--section`, not both.
    Ok(match (file_type, name, section) {
        (FileType::Spec, None, None) => {
            saved(document, engine.spec(path).cancel_with(token).run(), token)
        }
        (FileType::Project, Some(name), _) => {
            match engine.project(path).cancel_with(token).spec(name) {
                Ok(spec) => saved(document, spec.run(), token),
                Err(failure) => refusal(document, failure),
            }
        }
        // Every section, or the one named; an unknown section refuses the
        // run (D98).
        (FileType::Project, None, section) => {
            match project_request(&engine, path, section, token).run() {
                Ok(mut runs) => {
                    let saved = runs.save(token);
                    document.insert("members".into(), members(&runs));
                    result(
                        document,
                        &runs.summary(),
                        runs.status(),
                        runs.diagnostics(),
                        Some(saved),
                    )
                }
                Err(failure) => refusal(document, failure),
            }
        }
        (FileType::Spec, Some(_), _) => {
            return Err(not_taken(call, path, file_type, Some("with --spec")))
        }
        (FileType::Spec, None, Some(_)) => {
            return Err(not_taken(call, path, file_type, Some("with --section")))
        }
        (other, ..) => return Err(not_taken(call, path, other, None)),
    })
}

/// One specification's run, then its save, which polls `token`; a refusal of
/// the run names the specification's path.
fn saved(document: Document, run: Result<Run, Failure>, token: &CancellationToken) -> Printed {
    match run {
        Ok(mut run) => {
            let saved = run.save(token);
            result(
                document,
                &run.summary(),
                run.status(),
                run.diagnostics(),
                Some(saved),
            )
        }
        Err(failure) => refusal(document, failure),
    }
}

/// A project run's members in run order, as `cli.json.member_keys` gives
/// them, each named as its member line names it: by its listed name or, for
/// a listing that could not be read, by its path as the project file writes
/// it (D130).
fn members(runs: &ProjectRun) -> ContextValue {
    ContextValue::List(
        runs.members()
            .into_iter()
            .map(|(name, run)| {
                let output = run.output();
                let size = |measure: fn(&yamaa::Table) -> usize| {
                    output.map_or(ContextValue::Null, |table| count(measure(table)))
                };
                object([
                    ("name", text(name)),
                    ("status", text(run.status().as_str())),
                    ("rows", size(yamaa::Table::rows)),
                    ("columns", size(|table| table.columns().len())),
                    ("issue_count", count(run.diagnostics().len())),
                ])
            })
            .collect(),
    )
}

/// `yamaa submission PROJECT [--section SECTION] [--created TIME]`:
/// `p.submission(section, created)`, then `save()`. Without `--created`,
/// the submission reads the clock once for every section, and prints the
/// time it read after its summary line (D129).
fn submission(call: &Call, token: &CancellationToken) -> Result<Printed, Execution> {
    let path = path(call)?;
    let section = flag(call, "--section")?;
    let created = flag(call, "--created")?;
    let mut document = document(
        "submission",
        [
            ("path", text(path)),
            ("section", optional(section)),
            ("created", optional(created)),
            ("paths", ContextValue::List(Vec::new())),
        ],
    );
    let engine = engine();
    match engine.file_type(path) {
        Ok(FileType::Project) => {}
        Ok(other) => return Err(not_taken(call, path, other, None)),
        Err(failure) => return Ok(refusal(document, failure)),
    }
    let mut options = SubmissionOptions::new(yamaa::VERSION);
    options.created = created.map(str::to_owned);
    let composed = project_request(&engine, path, section, token).submission(&options);
    Ok(match composed {
        Ok(mut submission) => {
            let saved = submission.save(token);
            let used = submission.created().to_owned();
            document.insert("created".into(), text(&used));
            let summary = submission.summary();
            let issues = submission.diagnostics();
            // The time follows the summary line and any member line, before
            // the issues.
            let after = heading(&summary, &issues).len();
            let mut printed = result(document, &summary, submission.status(), issues, Some(saved));
            if created.is_none() {
                printed.lines.insert(after, format!("created: {used}"));
            }
            printed
        }
        // An unknown section refuses the submission (D98).
        Err(failure) => refusal(document, failure),
    })
}

/// `yamaa resolve PATH`: `s.resolve()` of a spec, or, with `--spec NAME`,
/// `p.spec(name).resolve()` of a project, printed as canonical JSON in both
/// formats. A project has no resolved form of its own, so it needs
/// `--spec`.
fn resolve(call: &Call) -> Result<Printed, Execution> {
    let path = path(call)?;
    let name = flag(call, "--spec")?;
    let document = document("resolve", [("path", text(path)), ("spec", optional(name))]);
    let engine = engine();
    let file_type = match engine.file_type(path) {
        Ok(file_type) => file_type,
        Err(failure) => return Ok(refusal(document, failure)),
    };
    let resolved = match (file_type, name) {
        (FileType::Spec, None) => engine.spec(path).resolve(),
        (FileType::Project, Some(name)) => engine
            .project(path)
            .spec(name)
            .and_then(|spec| spec.resolve()),
        (FileType::Spec, Some(_)) => {
            return Err(not_taken(call, path, file_type, Some("with --spec")))
        }
        (FileType::Project, None) => {
            return Err(not_taken(call, path, file_type, Some("without --spec")))
        }
        (other, _) => return Err(not_taken(call, path, other, None)),
    };
    Ok(match resolved {
        Ok(value) => verbatim(value.to_json()),
        Err(failure) => refusal(document, failure),
    })
}

/// JSON text printed as it is in both formats, as `resolve` and `schema`
/// print theirs, with exit status 0.
fn verbatim(json: String) -> Printed {
    Printed {
        lines: vec![json.clone()],
        json,
        status: SUCCEEDED,
    }
}

/// `yamaa read-odm SOURCE OUT`: `read_odm(source, ...)` with the archive
/// member and bounds given, then the table saved to OUT, whose extension
/// selects the csv or parquet profile (D134). It prints the one path the save
/// wrote. A refused read names SOURCE and a refused save names OUT, each as
/// given (D39); a refused save writes nothing. The JSON `rows` counts the
/// items read, so it is null only when the read is refused, and the `status`
/// follows the read: `succeeded` once the items are read, even when the save
/// is then refused, which its issues and exit status 1 report, and `failed`
/// only for a refused read (D169).
fn read_odm(call: &Call) -> Result<Printed, Execution> {
    let source = utf8(call, "SOURCE", &call.arguments[0])?;
    let out = utf8(call, "OUT", &call.arguments[1])?;
    let mut options = OdmOptions::new();
    if let Some(member) = flag(call, "--archive-member")? {
        options = options.with_archive_member(member);
    }
    // A bound beyond the platform's `usize` saturates; none of the shipped
    // targets has one narrower than the 64 bits the arguments admit.
    let wide = |limit: u64| usize::try_from(limit).unwrap_or(usize::MAX);
    if let Some(limit) = call.count("--max-archive-members") {
        options = options.with_max_archive_members(wide(limit));
    }
    if let Some(limit) = call.count("--max-expanded-bytes") {
        options = options.with_max_expanded_bytes(limit);
    }
    if let Some(limit) = call.count("--max-xml-depth") {
        options = options.with_max_xml_depth(wide(limit));
    }
    let mut document = document(
        "read-odm",
        [
            ("path", text(source)),
            ("rows", ContextValue::Null),
            ("paths", ContextValue::List(Vec::new())),
        ],
    );
    let engine = engine();
    let table = match engine.read_odm(source, &options) {
        Ok(table) => table,
        Err(failure) => return Ok(refusal(document, failure)),
    };
    document.insert("rows".into(), count(table.rows()));
    Ok(match engine.save_table(&table, out) {
        Ok(paths) => {
            document.insert("status".into(), text(Status::Succeeded.as_str()));
            document.insert("issues".into(), ContextValue::List(Vec::new()));
            document.insert("paths".into(), texts(&paths));
            Printed {
                lines: paths,
                json: ContextValue::Map(document).to_json(),
                status: SUCCEEDED,
            }
        }
        // The read completed, so its result succeeded: the refused save
        // prints D39's error text and exits 1, and its issues are the JSON
        // `issues` (D169).
        Err(failure) => refusal_with(document, Status::Succeeded, failure),
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
                json: ContextValue::Map(document).to_json(),
                status: exit(status),
            }
        }
        Err(failure) => refusal(document, failure),
    })
}

/// `yamaa schema TYPE`: `schema(file_type)`, the JSON Schema of a file type
/// as the canonical JSON text the facade gives, printed as it is in both
/// formats (D149). TYPE goes to the facade as given: one that names none of
/// the four file types is the facade's refusal, `unexpected_file_type`
/// (SCHEMA-2). Its JSON holds the command, the status and the issue, and
/// echoes TYPE as given as `type`, as a refused `resolve` echoes its `path`
/// and `spec` (D169).
fn schema(call: &Call) -> Result<Printed, Execution> {
    let file_type = utf8(call, "TYPE", &call.arguments[0])?;
    Ok(match yamaa::schema(file_type) {
        Ok(json) => verbatim(json),
        Err(failure) => refusal(document("schema", [("type", text(file_type))]), failure),
    })
}

/// `yamaa conditions`: `conditions()`, the condition catalogue (#3700). It
/// prints the facade's table lines: the column names, then one line per
/// condition. Its JSON holds the status, which nothing can make `failed`,
/// and each condition with `cli.json.condition_keys`, in name order. It
/// exits 0.
fn conditions() -> Printed {
    let catalogue = yamaa::conditions();
    let mut document = document("conditions", []);
    document.insert("status".into(), text(Status::Succeeded.as_str()));
    document.insert(
        "conditions".into(),
        ContextValue::List(
            catalogue
                .entries()
                .iter()
                .map(Condition::to_value)
                .collect(),
        ),
    );
    Printed {
        lines: catalogue.lines(),
        json: ContextValue::Map(document).to_json(),
        status: SUCCEEDED,
    }
}

/// `yamaa selftest`: `selftest()`, for the command line, which runs on no
/// interpreter (D28, D108, D123). It prints the facade's report line for
/// line, the same words as the hosts (D37, D119); its JSON holds the status,
/// each case with `cli.json.case_keys` in list order, and the engine
/// information. It exits 0 when no case failed: a case this build does not
/// cover is `skipped`, which fails nothing (D119).
fn selftest() -> Printed {
    let selftest = yamaa::selftest(Host::Cli, None);
    let cases = selftest
        .cases()
        .iter()
        .map(|case| {
            object([
                ("case", text(case.case)),
                ("status", text(case.status.as_str())),
                ("message", optional(case.message.as_deref())),
            ])
        })
        .collect();
    let mut document = document("selftest", []);
    document.insert("status".into(), text(selftest.status().as_str()));
    document.insert("cases".into(), ContextValue::List(cases));
    document.insert("info".into(), info(selftest.info()));
    Printed {
        lines: selftest.summary().lines().map(str::to_owned).collect(),
        json: ContextValue::Map(document).to_json(),
        status: exit(selftest.status()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lazy specification's summary, which stands in for a result's: its
    /// one line, `yamaa spec p.yaml`, and no member or issue line.
    fn summary() -> Summary {
        Summary::spec("p.yaml")
    }

    fn rendered(printed: Printed, format: Format) -> (String, u8) {
        match printed.render(format) {
            Execution::Ran { stdout, status } => (stdout, status),
            other => panic!("not run: {other:?}"),
        }
    }

    #[test]
    fn a_result_prints_its_lines_then_its_paths_and_exits_by_its_status() {
        let summary = summary();
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
            rendered(
                printed(Status::Succeeded, Some(saved.clone())),
                Format::Text
            ),
            (
                "yamaa spec p.yaml\n/s/dm-report.csv\n/s/dm.csv\n".to_owned(),
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
        // A check the token already cancelled reads nothing and refuses with
        // the cancellation's error text alone.
        let token = CancellationToken::new();
        token.cancel();
        let failure = engine()
            .spec("odm.xml")
            .cancel_with(&token)
            .check()
            .err()
            .expect("a check the token cancelled fails");
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
