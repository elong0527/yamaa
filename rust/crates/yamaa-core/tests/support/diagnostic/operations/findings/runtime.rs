//! What a project's functions and a run report: a project function's
//! invocation, binding and conformance failures (FUNC-2, FUNC-5, FUNC-8,
//! FUNC-9, FUNC-12), the lock (FUNC-3), and a run's failed output keys and
//! verifications (OUTPUT-3, CHECK-10 to CHECK-13).
use super::{column_type, document, text, value, value_type};
use crate::support::{
    diagnostic::{json::Json, Record},
    project_call_compiler,
};
use yamaa_core::{
    dataset::{CheckRecord, RowIdentity},
    diagnostic::{
        lock_diagnostics, ConditionCode, Diagnostic, LockFinding, LockOrigins, LockReason,
    },
    function_invocation::{FailureKind, InvocationFailure},
    function_signature::ProjectFunctionIdentity,
    project_activation::{BindingFailure, TestFailure},
    project_diagnostics::{self as projection, DefinitionSource, HostDetails},
    project_function::Function,
    verification_diagnostics::{project, Phase},
};

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "function_invocation" => function_invocation(input),
        "function_binding" => function_binding(input),
        "function_conformance" => function_conformance(input),
        "lock_mismatch" => lock_mismatch(input),
        "verification_projection" => verification_projection(input),
        _ => return None,
    })
}

/// How the host classified what it raised, `{"exception": {class, message}}`.
fn host(json: &Json) -> HostDetails<'_> {
    match json.members() {
        [(kind, facts)] if kind == "exception" => {
            let facts = facts.closed(&["class", "message"]);
            HostDetails::Exception {
                class: text(facts, "class"),
                message: text(facts, "message"),
            }
        }
        other => panic!("no host details {other:?}"),
    }
}

/// The failure of invoking the project function `function`, `{name, call}`,
/// at `path` while no row is derived, `{path, function, failure, host}`:
/// `failure` is `{"unknown_arguments": [...]}`, `{"missing_required":
/// {parameter}}`, `{"argument_type": {parameter, expected, actual}}`,
/// `{"result_type": {expected, actual}}`, `"call_failed"`,
/// `"invalid_host_result"`, `"boolean_result"` or `"undeclared_missing"`, and
/// `host` classifies what the host raised.
fn function_invocation(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["path", "function", "failure", "host"]);
    let function = input.field("function").closed(&["name", "call"]);
    let kind = match input.field("failure") {
        Json::String(kind) => match kind.as_str() {
            "call_failed" => FailureKind::CallFailed(()),
            "invalid_host_result" => FailureKind::InvalidHostResult(()),
            "boolean_result" => FailureKind::BooleanResult,
            "undeclared_missing" => FailureKind::UndeclaredMissing,
            other => panic!("no invocation failure {other:?}"),
        },
        failure => match failure.members() {
            [(kind, names)] if kind == "unknown_arguments" => FailureKind::UnknownArguments(
                names
                    .items()
                    .iter()
                    .map(|name| name.text().to_owned())
                    .collect(),
            ),
            [(kind, facts)] if kind == "missing_required" => FailureKind::MissingRequired {
                parameter: text(facts.closed(&["parameter"]), "parameter").into(),
            },
            [(kind, facts)] if kind == "argument_type" => {
                let facts = facts.closed(&["parameter", "expected", "actual"]);
                FailureKind::ArgumentType {
                    parameter: text(facts, "parameter").into(),
                    expected: value_type(facts.field("expected")),
                    actual: value_type(facts.field("actual")),
                }
            }
            [(kind, facts)] if kind == "result_type" => {
                let facts = facts.closed(&["expected", "actual"]);
                FailureKind::ResultType {
                    expected: column_type(facts.field("expected")),
                    actual: value_type(facts.field("actual")),
                }
            }
            other => panic!("no invocation failure {other:?}"),
        },
    };
    let failure = InvocationFailure {
        identity: ProjectFunctionIdentity {
            name: text(function, "name").into(),
            call: text(function, "call").into(),
        },
        kind,
    };
    let host = input.get("host").map(host);
    vec![
        projection::invocation(text(input, "path"), &failure, None, &[], host)
            .expect("a complete context")
            .record(),
    ]
}

/// The support's project function `function`, defined at `path`, as the one
/// definition of a project.
fn definition<'a>(function: &'a Function, path: &'a str) -> [DefinitionSource<'a>; 1] {
    [DefinitionSource { function, path }]
}

/// A callable its host cannot bind, of the support's project function
/// `function` defined at `path`, `{function, path, host}`.
fn function_binding(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["function", "path", "host"]);
    let function = project_call_compiler::function(text(input, "function"));
    let failures = [BindingFailure {
        function: 0,
        error: (),
    }];
    projection::binding_diagnostics(
        &definition(&function, text(input, "path")),
        &failures,
        &[Some(host(input.field("host")))],
    )
    .expect("a complete context")
    .iter()
    .map(Record::record)
    .collect()
}

/// The conformance case at index `case` of the support's project function
/// `function`, defined at `path`, whose result was `actual` rather than the
/// result the case declares, `{function, path, case, actual}`.
fn function_conformance(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["function", "path", "case", "actual"]);
    let function = project_call_compiler::function(text(input, "function"));
    let case = input.field("case").integer();
    let failures: [TestFailure<()>; 1] = [TestFailure::Result {
        function: 0,
        case,
        actual: value(input.field("actual")),
        expected: function.definition().tests[case].result.clone(),
    }];
    projection::test_diagnostics(
        &definition(&function, text(input, "path")),
        &failures,
        &[None],
    )
    .expect("a complete context")
    .iter()
    .map(Record::record)
    .collect()
}

/// The report of the host packaging tool's classified `findings` against the
/// lock `origins` declare, `{origins, findings}`: `origins` is `{source,
/// entry, field, lock_source, written}`, and each finding `{package, reason,
/// expected, actual}`, `actual` `null` when no version is installed.
fn lock_mismatch(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["origins", "findings"]);
    let origins =
        input
            .field("origins")
            .closed(&["source", "entry", "field", "lock_source", "written"]);
    let findings = input
        .field("findings")
        .items()
        .iter()
        .map(|finding| {
            let finding = finding.closed(&["package", "reason", "expected", "actual"]);
            LockFinding {
                package: text(finding, "package").into(),
                reason: match text(finding, "reason") {
                    "distribution_not_identified" => LockReason::DistributionNotIdentified,
                    "version_not_locked" => LockReason::VersionNotLocked,
                    "ambiguous_lock_version" => LockReason::AmbiguousLockVersion,
                    "invalid_locked_version" => LockReason::InvalidLockedVersion,
                    "package_not_installed" => LockReason::PackageNotInstalled,
                    "invalid_installed_version" => LockReason::InvalidInstalledVersion,
                    "version_mismatch" => LockReason::VersionMismatch,
                    other => panic!("no lock reason {other:?}"),
                },
                expected: finding
                    .field("expected")
                    .items()
                    .iter()
                    .map(|version| version.text().to_owned())
                    .collect(),
                actual: match finding.field("actual") {
                    Json::Null => None,
                    actual => Some(actual.text().to_owned()),
                },
            }
        })
        .collect::<Vec<_>>();
    let origins = LockOrigins {
        source: text(origins, "source"),
        entry: text(origins, "entry"),
        field: text(origins, "field"),
        lock_source: text(origins, "lock_source"),
        written: text(origins, "written"),
    };
    lock_diagnostics(Some(origins), &findings).expect("an admitted lock report")
}

/// What a run's check `records` report for the prepared `document` in the
/// `phase` named, `output` or `verification`, for the specification
/// `specification`, `{document, phase, specification, records}`: each record
/// is `{check, path, evaluated_count, failed_count, output_rows,
/// offending_rows}`, `check` naming the failed check (`missing_key`,
/// `duplicate_key`, `unique`, `all_or_none`, `assert` or `row_count`) and
/// each offending row the values of its keys, at position 0.
fn verification_projection(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["document", "phase", "specification", "records"]);
    let prepared = document::prepared(input);
    let records = input
        .field("records")
        .items()
        .iter()
        .map(|record| {
            let record = record.closed(&[
                "check",
                "path",
                "evaluated_count",
                "failed_count",
                "output_rows",
                "offending_rows",
            ]);
            CheckRecord {
                path: text(record, "path").into(),
                code: match text(record, "check") {
                    "missing_key" => ConditionCode::OutputMissingKeyFailed,
                    "duplicate_key" => ConditionCode::OutputDuplicateKeyFailed,
                    "unique" => ConditionCode::VerificationUniqueFailed,
                    "all_or_none" => ConditionCode::VerificationAllOrNoneFailed,
                    "assert" => ConditionCode::VerificationAssertFailed,
                    "row_count" => ConditionCode::VerificationRowCountFailed,
                    other => panic!("no failed check {other:?}"),
                },
                evaluated_count: record.field("evaluated_count").integer(),
                failed_count: record.field("failed_count").integer(),
                output_rows: record.field("output_rows").integer(),
                offending_rows: record
                    .field("offending_rows")
                    .items()
                    .iter()
                    .map(|row| RowIdentity {
                        position: 0,
                        values: row.items().iter().map(value).collect(),
                    })
                    .collect(),
                codelist: None,
                inventory: None,
                row_count: None,
                entries: None,
                denominator: None,
            }
        })
        .collect::<Vec<_>>();
    let phase = match text(input, "phase") {
        "output" => Phase::Output,
        "verification" => Phase::Verification,
        other => panic!("no projection phase {other:?}"),
    };
    project(&prepared, &records, phase, text(input, "specification"))
        .expect("a projection of supported records")
        .diagnostics
        .iter()
        .map(Record::record)
        .collect()
}
