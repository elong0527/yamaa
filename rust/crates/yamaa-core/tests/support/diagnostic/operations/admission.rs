//! The admitted reports of a project environment's findings
//! (`admission.json`): each issue names the file that declares its subject,
//! the project file or a file it lists, and the declaration that names that
//! file (D8). Each operation calls one entry point of
//! `project_environment_diagnostics::projection` with the arguments its case
//! writes, and reports every issue of the report it admits.
use super::{
    codelist_source, language,
    registry::{draft, findings, scalar_finding},
    text, File,
};
use crate::support::{
    diagnostic::json::Json,
    roots::{meta, text as written},
};
use yamaa_core::{
    ast::{self, CodelistDeclaration, CodelistSource, FunctionDeclaration},
    diagnostic::Diagnostic,
    project_environment_diagnostics::projection::{self, Admission, Capture, Origin, ScalarSource},
};

pub(super) const FILES: &[File] = &[fixture!("admission.json")];

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "admit_admission" => admit_admission(input),
        "admit_root_scalars" => admit_root_scalars(input),
        "admit_scalar_sources" => admit_scalar_sources(input),
        _ => return None,
    })
}

/// The report of an environment's findings, `{root, project, draft,
/// function_origins, function_files, codelist_origins, captures, findings}`:
/// the project file's identity `root`, the [`project`] root it lowered to and
/// the environment `draft` it holds (`{language, lock, functions,
/// codelists}`, as `project_environment` reads one); the name of each of the
/// draft's definitions and the index in the project's `functions` of the file
/// holding it; the index in the project's `codelists` of each of the draft's
/// codelist sources; the files held, each a [`capture`]; and the findings, as
/// `project_environment` reads them.
fn admit_admission(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&[
        "root",
        "project",
        "draft",
        "function_origins",
        "function_files",
        "codelist_origins",
        "captures",
        "findings",
    ]);
    let project = project(input.field("project"));
    let draft = draft(
        input
            .field("draft")
            .closed(&["language", "lock", "functions", "codelists"]),
    );
    let findings = findings(input, &draft);
    let function_origins = input
        .field("function_origins")
        .items()
        .iter()
        .map(|name| name.text().to_owned())
        .collect::<Vec<_>>();
    let indexes = |key: &str| {
        input
            .field(key)
            .items()
            .iter()
            .map(Json::integer)
            .collect::<Vec<usize>>()
    };
    let function_files = indexes("function_files");
    let codelist_origins = indexes("codelist_origins");
    let captures = input
        .field("captures")
        .items()
        .iter()
        .map(|held| capture(held.closed(&["function_file", "codelist", "identity"])))
        .collect::<Vec<_>>();
    projection::admit_admission(
        Admission {
            project: &project,
            root: text(input, "root"),
            draft: &draft,
            function_origins: &function_origins,
            function_files: &function_files,
            codelist_origins: &codelist_origins,
            captures: captures.into_iter(),
        },
        &findings,
    )
    .expect("an admitted report")
    .diagnostics()
    .expect("a complete projection")
}

/// The report of the values the project file `root` writes in place that
/// their declared types cannot read, `{root, findings}`, each a scalar
/// finding as `function_file_scalars` reads one.
fn admit_root_scalars(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["root", "findings"]);
    let findings = input
        .field("findings")
        .items()
        .iter()
        .map(scalar_finding)
        .collect::<Vec<_>>();
    projection::admit_root_scalars(text(input, "root"), &findings)
        .expect("an admitted report")
        .diagnostics()
        .expect("a complete projection")
}

/// The report of the values held files write that their declared types
/// cannot read, `{root, project, sources}`: the project file's identity
/// `root`, its [`project`] root, and each failed file as a [`capture`] with
/// its `findings`, each a scalar finding as `function_file_scalars` reads
/// one.
fn admit_scalar_sources(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["root", "project", "sources"]);
    let project = project(input.field("project"));
    let sources = input
        .field("sources")
        .items()
        .iter()
        .map(|source| {
            let source = source.closed(&["function_file", "codelist", "identity", "findings"]);
            (
                capture(source),
                source
                    .field("findings")
                    .items()
                    .iter()
                    .map(scalar_finding)
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    projection::admit_scalar_sources(
        &project,
        text(input, "root"),
        sources.iter().map(|(capture, findings)| ScalarSource {
            capture: *capture,
            findings,
        }),
    )
    .expect("an admitted report")
    .diagnostics()
    .expect("a complete projection")
}

/// A held file, its `identity` and the declaration that names it: the
/// `function_file` the project lists at that index of its `functions`, or
/// the `codelist` source it declares at that index of its `codelists`.
#[track_caller]
fn capture(json: &Json) -> Capture<'_> {
    let origin = match (json.get("function_file"), json.get("codelist")) {
        (Some(index), None) => Origin::FunctionFile(index.integer()),
        (None, Some(index)) => Origin::Codelist(index.integer()),
        _ => panic!("a held file is named by one function file or one codelist: {json:?}"),
    };
    Capture {
        origin,
        identity: text(json, "identity"),
    }
}

/// A project root as a project file declares the files of its environment,
/// `{functions, environments, codelists}`: the paths of the function files it
/// lists, or `null` when it writes no list; the lock of each host language,
/// by language key, `{lock}`, none when it writes no `environments`; and its
/// codelist sources, each the path of a codelist file or a source written in
/// place ([`codelist_source`]). It writes the language version 1.0 and
/// declares no submission.
#[track_caller]
fn project(json: &Json) -> ast::Project {
    let json = json.closed(&["functions", "environments", "codelists"]);
    ast::Project {
        header: ast::Header {
            version: written("1.0"),
            meta: meta(),
        },
        functions: match json.field("functions") {
            Json::Null => None,
            files => Some(
                files
                    .items()
                    .iter()
                    .map(|file| FunctionDeclaration {
                        file: written(file.text()),
                        meta: meta(),
                    })
                    .collect(),
            ),
        },
        environments: json
            .get("environments")
            .map_or(&[][..], Json::members)
            .iter()
            .map(|(host, environment)| ast::HostEnvironment {
                language: language(host),
                lock: Some(written(text(environment.closed(&["lock"]), "lock"))),
                meta: meta(),
            })
            .collect(),
        codelists: json
            .field("codelists")
            .items()
            .iter()
            .map(|source| CodelistDeclaration {
                source: match source {
                    Json::String(path) => CodelistSource::File(written(path)),
                    inline => CodelistSource::Inline(Box::new(codelist_source(inline))),
                },
                meta: meta(),
            })
            .collect(),
        submission: Ok(Default::default()),
        paths: ast::Paths::new(),
        meta: meta(),
    }
}
