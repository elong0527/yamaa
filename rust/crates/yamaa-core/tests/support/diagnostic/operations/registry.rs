//! The findings of single core entry points (`registry.json`) and of a
//! project's environment (`environment.json`). Each operation calls one core
//! function with the arguments its case writes.
use super::{bindings, codelist_source, column_type, language, text, value, File};
use crate::support::{diagnostic::json::Json, project_call_compiler::definition};
use yamaa_core::{
    application_issue::{self, Fault, Step},
    ast, conversion,
    dataset::{Check, Rounding, WhereClause, WhereTerm},
    dataset_checks,
    diagnostic::Diagnostic,
    expression::{compile, Context},
    iso8601::Form,
    portable_regex::{CompileError, Pattern},
    project_environment::{self, Draft, LockKind, LockReference},
    project_environment_diagnostics,
    project_function_document::{Finding, Kind},
    resource::ResourceFailure,
    scalar_language,
    table::ValueRef,
};

pub(super) const FILES: &[File] = &[fixture!("environment.json"), fixture!("registry.json")];

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "application_issue" => vec![application_issue(input)],
        "column_diagnostic" => column_diagnostic(input),
        "convert" => convert(input),
        "evaluate_number" => evaluate_number(input),
        "function_file_scalars" => function_file_scalars(input),
        "incomparable_sources" => incomparable_sources(input),
        "input_resource" => input_resource(input),
        "invalid_packaging_lock" => invalid_packaging_lock(input),
        "matches_pattern" => matches_pattern(input),
        "not_missing" => not_missing(input),
        "project_environment" => project_environment(input),
        "round_half_away_from_zero" => round_half_away_from_zero(input),
        "save_target" => save_target(input),
        _ => return None,
    })
}

/// An application's refusal, `{issue, ...}`: `publication_failed` with the
/// `target` it could not publish, `unsupported` with the `operation` and the
/// `path` it is refused at, `rejected` with the engine's `step` and fault
/// `code`, and `lock_mismatch` with the lock's `field` and the `context` the
/// packaging tools classified.
fn application_issue(input: &Json) -> Diagnostic {
    match text(input, "issue") {
        "publication_failed" => {
            input.closed(&["issue", "target"]);
            application_issue::publication_failed(text(input, "target"))
        }
        "unsupported" => {
            input.closed(&["issue", "operation", "path"]);
            application_issue::unsupported(
                text(input, "operation"),
                input.get("path").map(Json::text),
            )
        }
        "rejected" => {
            input.closed(&["issue", "step", "code"]);
            let step = text(input, "step");
            let code = text(input, "code");
            application_issue::rejected(
                *Step::ALL
                    .iter()
                    .find(|known| known.as_str() == step)
                    .unwrap_or_else(|| panic!("no engine step {step:?}")),
                *Fault::ALL
                    .iter()
                    .find(|known| known.as_str() == code)
                    .unwrap_or_else(|| panic!("no engine fault {code:?}")),
            )
        }
        "lock_mismatch" => {
            input.closed(&["issue", "field", "context"]);
            application_issue::lock_mismatch(
                text(input, "field"),
                input
                    .field("context")
                    .members()
                    .iter()
                    .map(|(key, fact)| (key.clone(), fact.value()))
                    .collect(),
            )
        }
        other => panic!("no application issue {other:?}"),
    }
}

/// A resource failure as a case spells it.
#[track_caller]
fn resource_failure(json: &Json) -> ResourceFailure {
    match json.text() {
        "missing" => ResourceFailure::Missing,
        "not_regular_file" => ResourceFailure::NotRegularFile,
        "uri_scheme" => ResourceFailure::UriScheme,
        "backslash" => ResourceFailure::Backslash,
        "empty_segment" => ResourceFailure::EmptySegment,
        "symlink" => ResourceFailure::Symlink,
        "content_changed" => ResourceFailure::ContentChanged,
        other => panic!("no resource failure {other:?}"),
    }
}

/// The refusal of an input's declared path, `{failure, dataset, path}`, with
/// the path as written.
fn input_resource(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["failure", "dataset", "path"]);
    vec![resource_failure(input.field("failure"))
        .diagnostic(text(input, "dataset"), text(input, "path"))]
}

/// A save's refusal of a target its walk stops at, `{failure, field, path}`:
/// `field` declares the file, or is `null` for a path the caller names. Any
/// other failure is no walk's and reports none.
fn save_target(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["failure", "field", "path"]);
    let field = match input.field("field") {
        Json::Null => None,
        field => Some(field.text()),
    };
    resource_failure(input.field("failure"))
        .target_diagnostic(field, text(input, "path"))
        .into_iter()
        .collect()
}

/// The evaluation of a derivation's numeric `expression` written at `path`,
/// `{expression, path, bindings}`, where each name `bindings` gives reads its
/// value and every other name is absent.
fn evaluate_number(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["expression", "path", "bindings"]);
    let plan = compile(
        text(input, "expression"),
        Context::Derivation,
        text(input, "path"),
    )
    .expect("a derivation that compiles");
    match plan.evaluate_number(&mut bindings(input.field("bindings"))) {
        Ok(_) => Vec::new(),
        Err(error) => vec![error.diagnostic().expect("a language condition")],
    }
}

/// The conversion of `value` to the column type `to`, refused at `path`,
/// `{value, to, path}`.
fn convert(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["value", "to", "path"]);
    match conversion::convert(&value(input.field("value")), column_type(input.field("to"))) {
        Ok(_) => Vec::new(),
        Err(error) => vec![error.into_diagnostic(text(input, "path").into())],
    }
}

/// The finding of a `not_missing` verification of `column`, whose `values`
/// are the column's in row order, at `path`, `{values, path, column}`: it
/// counts the missing values, or reports none when no value is missing.
fn not_missing(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["values", "path", "column"]);
    let values = input
        .field("values")
        .items()
        .iter()
        .map(value)
        .collect::<Vec<_>>();
    dataset_checks::not_missing(&values)
        .diagnostic(text(input, "path").into(), text(input, "column").into())
        .into_iter()
        .collect()
}

/// The finding of a column `check` that `failures` rows of `column` fail, at
/// `path`, `{check, path, column, failures}`, or none when no row fails.
fn column_diagnostic(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["check", "path", "column", "failures"]);
    dataset_checks::column_diagnostic(
        &check(input.field("check")),
        text(input, "path").into(),
        text(input, "column").into(),
        input.field("failures").integer(),
    )
    .into_iter()
    .collect()
}

/// A column check as a case writes it: `"mandatory"`, `{"allowed_values":
/// [...]}`, `{"range": {min, max}}` with either bound, `{"max_length": n}`,
/// `{"data_type": form}`, `{"matches": pattern}`, `{"codelist": {id,
/// values}}`, `{"value_entries": [{id, where}, ...]}` and
/// `{"value_entry_match": {id, where}}`. A check that a scope may narrow has
/// none, so it reads every row.
#[track_caller]
fn check(json: &Json) -> Check {
    if let Json::String(name) = json {
        return match name.as_str() {
            "mandatory" => Check::Mandatory(None),
            other => panic!("no column check {other:?}"),
        };
    }
    let [(kind, facts)] = json.members() else {
        panic!("a column check is one member: {json:?}")
    };
    match kind.as_str() {
        "allowed_values" => Check::AllowedValues(facts.items().iter().map(value).collect()),
        "range" => {
            let facts = facts.closed(&["min", "max"]);
            Check::Range {
                min: facts.get("min").map(value),
                max: facts.get("max").map(value),
            }
        }
        "max_length" => Check::MaxLength(facts.integer()),
        "data_type" => Check::DataType {
            form: *Form::ALL
                .iter()
                .find(|form| form.name() == facts.text())
                .unwrap_or_else(|| panic!("no data type form {facts:?}")),
            scope: None,
        },
        "matches" => {
            Check::Matches(Pattern::compile(facts.text()).expect("a pattern that compiles"))
        }
        "codelist" => {
            let facts = facts.closed(&["id", "values"]);
            Check::Codelist {
                id: text(facts, "id").into(),
                values: facts.field("values").items().iter().map(value).collect(),
            }
        }
        "value_entries" => Check::ValueEntries(
            facts
                .items()
                .iter()
                .map(|entry| {
                    let entry = entry.closed(&["id", "where"]);
                    (text(entry, "id").into(), clause(entry.field("where")))
                })
                .collect(),
        ),
        "value_entry_match" => {
            let facts = facts.closed(&["id", "where"]);
            Check::ValueEntryMatch {
                id: text(facts, "id").into(),
                clause: clause(facts.field("where")),
            }
        }
        other => panic!("no column check {other:?}"),
    }
}

/// A value-level where clause, its terms each `{column, values}`: the
/// position of a where column and the values it lists.
#[track_caller]
fn clause(json: &Json) -> WhereClause {
    WhereClause {
        terms: json
            .items()
            .iter()
            .map(|term| {
                let term = term.closed(&["column", "values"]);
                WhereTerm::listed(
                    term.field("column").integer(),
                    term.field("values")
                        .items()
                        .iter()
                        .map(|listed| listed.text().into())
                        .collect(),
                )
            })
            .collect(),
    }
}

/// A `matches` verification's `pattern` written at `path`, `{pattern, path}`,
/// refused when it does not compile.
fn matches_pattern(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["pattern", "path"]);
    let pattern = text(input, "pattern");
    match Pattern::compile(pattern) {
        Ok(_) => Vec::new(),
        Err(CompileError::Invalid { .. }) => vec![dataset_checks::invalid_pattern_diagnostic(
            text(input, "path").into(),
            pattern.into(),
        )],
        Err(other) => panic!("a pattern refused with no language finding: {other:?}"),
    }
}

/// Rounding `value` half away from zero to `digits` places, from the
/// `source` its declaration at `path` names, `{source, path, digits, value}`.
fn round_half_away_from_zero(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["source", "path", "digits", "value"]);
    let rounding = Rounding::new(
        text(input, "source"),
        text(input, "path"),
        text(input, "digits"),
    )
    .expect("admitted digits");
    let value = value(input.field("value"));
    rounding
        .apply(ValueRef::from(&value))
        .err()
        .into_iter()
        .collect()
}

/// The refusal of the extremes of `sources`, whose `types` cannot be
/// compared, at `path`, `{sources, types, path}`.
fn incomparable_sources(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["sources", "types", "path"]);
    let sources = input
        .field("sources")
        .items()
        .iter()
        .map(|source| source.text().to_owned())
        .collect::<Vec<_>>();
    let types = input
        .field("types")
        .items()
        .iter()
        .map(column_type)
        .collect::<Vec<_>>();
    vec![scalar_language::incomparable_diagnostic(
        &sources,
        &types,
        text(input, "path"),
    )]
}

/// The findings of a project's environment, each at its path, `{language,
/// lock, functions, codelists}`: the host `language`, or `null` for none; the
/// host's `lock`, `{written, kind}`, or `null`; the function files the project
/// lists, or `null` when it writes no list, each the support's integer
/// identity `{identity, implementations}` with its callables as `[language,
/// callable]` pairs; and the codelist sources, each as a codelist file
/// writes it ([`codelist_source`]).
fn project_environment(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["language", "lock", "functions", "codelists"]);
    let draft = Draft {
        language: match input.field("language") {
            Json::Null => None,
            host => Some(language(host.text())),
        },
        lock: match input.field("lock") {
            Json::Null => None,
            lock => {
                let lock = lock.closed(&["written", "kind"]);
                Some(LockReference {
                    written: text(lock, "written").into(),
                    kind: match text(lock, "kind") {
                        "uv" => LockKind::Uv,
                        "renv" => LockKind::Renv,
                        other => panic!("no lock kind {other:?}"),
                    },
                })
            }
        },
        functions: match input.field("functions") {
            Json::Null => None,
            files => Some(files.items().iter().map(function_file).collect()),
        },
        codelists: input
            .field("codelists")
            .items()
            .iter()
            .map(codelist_source)
            .collect(),
    };
    let findings = project_environment::validate(&draft);
    project_environment_diagnostics::diagnostics(&draft, &findings).expect("a complete projection")
}

/// A function file defining the support's integer identity `identity`,
/// `{identity, implementations}`.
#[track_caller]
fn function_file(json: &Json) -> ast::Function {
    let json = json.closed(&["identity", "implementations"]);
    let implementations = json
        .field("implementations")
        .items()
        .iter()
        .map(|implementation| match implementation.items() {
            [host, callable] => (language(host.text()), callable.text()),
            other => panic!("an implementation is a [language, callable] pair: {other:?}"),
        })
        .collect::<Vec<_>>();
    definition(text(json, "identity")).file(&implementations)
}

/// The findings of a function file's values that their declared types cannot
/// read, `{findings}`, each `{path, node, kind, value}` with the text the
/// value is written as.
fn function_file_scalars(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["findings"]);
    let findings = input
        .field("findings")
        .items()
        .iter()
        .map(|finding| {
            let finding = finding.closed(&["path", "node", "kind", "value"]);
            Finding {
                path: text(finding, "path").into(),
                node: finding.field("node").integer(),
                kind: match text(finding, "kind") {
                    "normalized_shape" => Kind::NormalizedShape,
                    "integer_range" => Kind::IntegerRange,
                    "invalid_date" => Kind::InvalidDate,
                    "invalid_datetime" => Kind::InvalidDateTime,
                    other => panic!("no function file value finding {other:?}"),
                },
                value: finding.get("value").map(|written| written.text().into()),
            }
        })
        .collect::<Vec<_>>();
    project_environment_diagnostics::function_file_scalar_diagnostics(&findings)
        .expect("a complete projection")
}

/// The refusal of a `language` host's lock that is no packaging lock,
/// `{language, project, written}`: the project file that declares it and the
/// lock as written.
fn invalid_packaging_lock(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["language", "project", "written"]);
    vec![project_environment_diagnostics::invalid_packaging_lock(
        language(text(input, "language")),
        text(input, "project"),
        text(input, "written"),
    )]
}
