//! Complete project reports borrow one owned attempt. Only explicit ordinary
//! host facts are portable; original opaque payloads and interrupts stay held.
use crate::{
    issue_rows::Issue,
    project_activation_diagnostics::{self as activation, DefinitionSource},
    project_function_diagnostics::{self as invocation, HostDetails},
    project_run::{Attempt, BoundaryFailure, PreparedRun},
    scalar_transport::ScalarValue,
    specification_report::{self as report, BuildResult, Identity},
};
use serde_json::{json, Value};
use yamaa_engine::{
    dataset::{CheckRecord, ExecutionError},
    function_invocation::FailureKind,
    project_activation::{Failure, TestFailure},
    specification_run::{PortError, RunError},
};

/// Facts refer to the exact payload in this attempt, never to an error string or
/// an ordinal supplied by a host. No classifier callback receives authority.
/// Address-based facts require a non-zero-sized host payload; opaque zero-sized
/// failures remain held and can be reported only without classification facts.
pub enum Classified<'a, E> {
    Lock {
        failure: &'a E,
        findings: &'a [crate::project_lock::Finding],
    },
    Host {
        failure: &'a E,
        details: HostDetails<'a>,
    },
}
impl<E> Classified<'_, E> {
    pub(crate) fn failure(&self) -> &E {
        match self {
            Self::Lock { failure, .. } | Self::Host { failure, .. } => failure,
        }
    }
}

#[derive(Debug)]
pub enum Error<'a, C, E> {
    Report(report::Error),
    Opaque(&'a E),
    Boundary(&'a BoundaryFailure<E>),
    Source(&'a PortError<C, crate::specification_run::Error>),
    Execution(&'a ExecutionError<E>),
}
impl<C, E> From<report::Error> for Error<'_, C, E> {
    fn from(error: report::Error) -> Self {
        Self::Report(error)
    }
}
fn invalid<'a, C, E>() -> Error<'a, C, E> {
    Error::Report(report::Error::InvalidObservation)
}
type FactMap<'a, 'f, E> = std::collections::BTreeMap<*const E, &'f Classified<'a, E>>;
fn host<'a, E>(facts: &FactMap<'a, '_, E>, error: &E) -> Option<HostDetails<'a>> {
    match facts.get(&std::ptr::from_ref(error)) {
        Some(Classified::Host { details, .. }) => Some(*details),
        _ => None,
    }
}
fn invocation_host<'a, E>(
    facts: &FactMap<'a, '_, E>,
    failure: &FailureKind<E>,
) -> Option<HostDetails<'a>> {
    match failure {
        FailureKind::CallFailed(error) | FailureKind::InvalidHostResult(error) => {
            host(facts, error)
        }
        _ => None,
    }
}
pub(crate) fn diagnostic(issue: Issue) -> Result<Value, report::Error> {
    let context: Value =
        serde_json::from_str(&issue.context).map_err(|_| report::Error::InvalidObservation)?;
    Ok(
        json!({"phase":issue.phase,"condition":issue.condition,"requirement":issue.requirement,"spec_paths":issue.spec_paths,"context":context}),
    )
}
fn project_error<'a, C, E>(error: invocation::Error<'a, E>) -> Error<'a, C, E> {
    match error {
        invocation::Error::Opaque(error) => Error::Opaque(error),
        invocation::Error::Limit => report::Error::OutputLimit.into(),
        invocation::Error::InvalidContext => invalid(),
    }
}
fn checks(records: &[CheckRecord], phase: &str) -> Value {
    let scalar = |value: &yamaa_core::value::Value| json!(ScalarValue::from_core(value.clone()));
    json!({"phase":phase,"verifications":records.iter().map(|record| {
        let mut value = json!({"spec_path":record.path,"condition":record.condition,"requirement":record.requirement,
            "evaluated_count":record.evaluated_count.to_string(),"failed_count":record.failed_count.to_string(),"output_rows":record.output_rows.to_string(),
            "offending_rows":record.offending_rows.iter().map(|row| json!({"position":row.position.to_string(),"keys":row.values.iter().map(scalar).collect::<Vec<_>>()})).collect::<Vec<_>>()});
        if let Some(codelist) = &record.codelist { value["codelist"] = json!({"id":codelist.id,"values":codelist.values.iter().map(scalar).collect::<Vec<_>>()}); }
        value
    }).collect::<Vec<_>>()})
}
pub(crate) fn checked_records(
    run: &dyn crate::specification_run_view::RunView,
    records: &[CheckRecord],
    phase: &str,
    id: &Identity<'_>,
) -> Result<(Vec<Value>, Vec<Value>), report::Error> {
    crate::project_codelist_diagnostics::codelist_issues(run.compiled(), records).map_err(
        |error| match error {
            crate::project_codelist_diagnostics::Error::Limit => report::Error::OutputLimit,
            _ => report::Error::InvalidObservation,
        },
    )?;
    report::check_observations(run, &checks(records, phase), id)
}

fn same_value(left: &yamaa_core::value::Value, right: &yamaa_core::value::Value) -> bool {
    match (left, right) {
        (yamaa_core::value::Value::Float(a), yamaa_core::value::Value::Float(b)) => {
            a.get().to_bits() == b.get().to_bits()
        }
        _ => left == right,
    }
}
/// The prefix formatter also supports incomplete attempts. A complete report
/// additionally requires the boundary result and each actual observation to agree.
pub(crate) enum ActivationBoundary<'a, E> {
    Complete,
    Failed(&'a Failure<E>),
    Incomplete,
}
pub(crate) fn validate_activation<E>(
    run: &dyn crate::project_activation_observations::ActivationView,
    observed: &yamaa_engine::project_activation::Observations,
    boundary: ActivationBoundary<'_, E>,
) -> Result<(), report::Error> {
    use yamaa_engine::project_activation::{BindingOutcome, LockObservation, TestOutcome};
    let selected = run.called_functions();
    let total = selected
        .iter()
        .map(|&index| {
            run.environment().functions()[index]
                .definition()
                .tests
                .len()
        })
        .sum::<usize>();
    let bound = observed.lock == LockObservation::Verified
        && observed.bindings.len() == selected.len()
        && observed
            .bindings
            .iter()
            .all(|binding| binding.outcome == BindingOutcome::Bound);
    match boundary {
        ActivationBoundary::Complete => {
            if !selected.is_empty()
                && (!bound
                    || observed.tests.len() != total
                    || observed
                        .tests
                        .iter()
                        .any(|case| case.outcome != TestOutcome::Passed))
            {
                return Err(report::Error::InvalidObservation);
            }
        }
        ActivationBoundary::Failed(Failure::Lock(_)) => {
            if observed.lock != LockObservation::Rejected
                || !observed.bindings.is_empty()
                || !observed.tests.is_empty()
            {
                return Err(report::Error::InvalidObservation);
            }
        }
        ActivationBoundary::Failed(Failure::Bindings(failures)) => {
            if failures.is_empty()
                || observed.lock != LockObservation::Verified
                || observed.bindings.len() != selected.len()
                || !observed.tests.is_empty()
                || observed.bindings.iter().any(|binding| {
                    !matches!(
                        binding.outcome,
                        BindingOutcome::Bound | BindingOutcome::Rejected
                    )
                })
                || failures.len()
                    != observed
                        .bindings
                        .iter()
                        .filter(|binding| binding.outcome == BindingOutcome::Rejected)
                        .count()
            {
                return Err(report::Error::InvalidObservation);
            }
            for failure in failures {
                if observed
                    .bindings
                    .get(failure.function)
                    .is_none_or(|binding| binding.outcome != BindingOutcome::Rejected)
                {
                    return Err(report::Error::InvalidObservation);
                }
            }
        }
        ActivationBoundary::Failed(Failure::Tests(failures)) => {
            if failures.is_empty()
                || !bound
                || observed.tests.len() != total
                || failures.len()
                    != observed
                        .tests
                        .iter()
                        .filter(|case| case.outcome != TestOutcome::Passed)
                        .count()
            {
                return Err(report::Error::InvalidObservation);
            }
            let cases = observed
                .tests
                .iter()
                .map(|observation| ((observation.function, observation.case), observation))
                .collect::<std::collections::BTreeMap<_, _>>();
            for failure in failures {
                let (function, case) = match failure {
                    TestFailure::Invocation { function, case, .. }
                    | TestFailure::Result { function, case, .. } => (*function, *case),
                };
                let Some(observation) = cases.get(&(function, case)) else {
                    return Err(report::Error::InvalidObservation);
                };
                match failure {
                    TestFailure::Invocation { .. }
                        if observation.outcome == TestOutcome::InvocationFailed
                            && observation.actual.is_none() => {}
                    TestFailure::Result { actual, .. }
                        if observation.outcome == TestOutcome::ResultMismatch
                            && observation
                                .actual
                                .as_ref()
                                .is_some_and(|held| same_value(held, actual)) => {}
                    _ => return Err(report::Error::InvalidObservation),
                }
            }
        }
        _ => {}
    }
    for case in &observed.tests {
        if case.outcome != TestOutcome::Passed {
            continue;
        }
        let function = &run.environment().functions()[selected[case.function]];
        let definition = function.definition();
        let expected = &definition.tests[case.case].result;
        if !case.actual.as_ref().is_some_and(|actual| {
            yamaa_core::project_function_result::results_match(
                actual,
                expected,
                definition.comparison_decimals,
            )
        }) {
            return Err(report::Error::InvalidObservation);
        }
    }
    Ok(())
}

pub(crate) fn activation_origins(
    run: &dyn crate::project_activation_observations::ActivationView,
    paths: &[String],
    issues: Vec<Issue>,
    functions: impl Iterator<Item = usize> + Clone,
) -> Result<Vec<Value>, report::Error> {
    // Admit the complete origin lookup and temporary ownership before collecting
    // indices or captures. Both ordinary runs and graph unions are sealed views.
    let mut budget =
        crate::report_projection::Budget::new(crate::specification_check::MAX_ISSUE_BYTES);
    let captured = run.captured_environment();
    budget.entries(captured.captures().len())?;
    budget.entries(paths.len())?;
    for selected in functions.clone() {
        budget.entries(1)?;
        budget.work(captured.captures().len())?;
        budget.text(&captured.root().source().identity)?;
        budget.text(
            paths
                .get(selected)
                .ok_or(report::Error::InvalidObservation)?,
        )?;
        for capture in captured.captures() {
            budget.text(&capture.document.source().identity)?;
        }
    }
    let functions = functions.collect::<Vec<_>>();
    if issues.len() != functions.len() {
        return Err(report::Error::InvalidObservation);
    }
    let entry = captured.root().source().identity.as_str();
    let external = captured
        .captures()
        .iter()
        .filter_map(|capture| match &capture.origin {
            crate::project_source::Origin::Function(name) => {
                Some((name.as_str(), capture.document.source().identity.as_str()))
            }
            _ => None,
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut remaining = crate::specification_check::MAX_ISSUE_BYTES;
    let mut metadata = Vec::new();
    for &selected in &functions {
        let &index = run
            .called_functions()
            .get(selected)
            .ok_or(report::Error::InvalidObservation)?;
        let name = &run
            .environment()
            .functions()
            .get(index)
            .ok_or(report::Error::InvalidObservation)?
            .definition()
            .name;
        if captured.origins().functions.get(index) != Some(name) {
            return Err(report::Error::InvalidObservation);
        }
        let declaration = paths
            .get(selected)
            .ok_or(report::Error::InvalidObservation)?;
        let external = external.get(name.as_str()).copied();
        let source = external.unwrap_or(entry);
        for text in [source, entry, declaration] {
            remaining = remaining
                .checked_sub(
                    text.len()
                        .checked_mul(12)
                        .and_then(|size| size.checked_add(128))
                        .ok_or(report::Error::OutputLimit)?,
                )
                .ok_or(report::Error::OutputLimit)?;
        }
        metadata.push((source, declaration, external.is_some()));
    }
    if !crate::issue_rows::within_limit(&issues, remaining) {
        return Err(report::Error::OutputLimit);
    }
    issues
        .into_iter()
        .zip(metadata)
        .map(|(issue, (source, declaration, external))| {
            let mut finding = diagnostic(issue)?;
            if external {
                let paths = finding["spec_paths"]
                    .as_array_mut()
                    .ok_or(report::Error::InvalidObservation)?;
                for path in paths {
                    let suffix = path
                        .as_str()
                        .and_then(|path| path.strip_prefix(declaration))
                        .and_then(|suffix| suffix.strip_prefix('.'))
                        .ok_or(report::Error::InvalidObservation)?;
                    *path = json!(suffix);
                }
            }
            let context = finding["context"]
                .as_object_mut()
                .ok_or(report::Error::InvalidObservation)?;
            context.insert("source".into(), json!(source));
            context.insert("entry".into(), json!(entry));
            context.insert("environment_path".into(), json!(declaration));
            Ok(finding)
        })
        .collect()
}

/// Neither reporting nor obtaining output invokes a port. Publication remains
/// the same explicit successful-output gate used by ordinary specification runs.
pub fn build_result<'a, C, E>(
    run: &PreparedRun,
    attempt: &'a Attempt<C, E>,
    id: Identity<'_>,
    facts: &[Classified<'a, E>],
) -> Result<BuildResult, Error<'a, C, E>> {
    if facts.len() > 65_536 {
        return Err(report::Error::OutputLimit.into());
    }
    // Distinct zero-sized values can share an address. Never grant one fact
    // authority over another held failure merely because their pointers agree.
    if std::mem::size_of::<E>() == 0 && !facts.is_empty() {
        return Err(invalid());
    }
    let mut held = std::collections::BTreeSet::new();
    crate::project_attempt::visit_host_failures(attempt, |_, error| {
        held.insert(std::ptr::from_ref(error));
    });
    let mut supplied = std::collections::BTreeMap::new();
    for fact in facts {
        let pointer = std::ptr::from_ref(fact.failure());
        if !held.contains(&pointer) || supplied.insert(pointer, fact).is_some() {
            return Err(invalid());
        }
        match fact {
            Classified::Lock { failure, .. } => {
                if !matches!(&attempt.boundary, Err(BoundaryFailure::Activation(Failure::Lock(error))) if std::ptr::eq(*failure,error))
                {
                    return Err(invalid());
                }
            }
            Classified::Host { failure, .. } => {
                if matches!(&attempt.boundary, Err(BoundaryFailure::Activation(Failure::Lock(error))) if std::ptr::eq(*failure,error))
                {
                    return Err(invalid());
                }
            }
        }
    }
    let activation = crate::project_activation_observations::activation(run, &attempt.activation)
        .map_err(|error| match error {
        crate::project_activation_observations::Error::Limit => {
            Error::Report(report::Error::OutputLimit)
        }
        _ => invalid(),
    })?;
    let boundary = match &attempt.boundary {
        Ok(()) => ActivationBoundary::Complete,
        Err(BoundaryFailure::Activation(error)) => ActivationBoundary::Failed(error),
        _ => ActivationBoundary::Incomplete,
    };
    validate_activation(run, &attempt.activation, boundary)?;
    let execution = crate::project_attempt::execution(attempt);
    let mut diagnostics = Vec::new();
    let mut verifications = Vec::new();
    let mut result;
    if let Err(boundary) = &attempt.boundary {
        let BoundaryFailure::Activation(failure) = boundary else {
            return Err(Error::Boundary(boundary));
        };
        let paths = run
            .compiled()
            .called_functions()
            .iter()
            .map(|&index| {
                format!(
                    "functions.{}",
                    run.environment().functions()[index].definition().name
                )
            })
            .collect::<Vec<_>>();
        let definitions = run
            .compiled()
            .called_functions()
            .iter()
            .zip(&paths)
            .map(|(&index, path)| DefinitionSource {
                function: &run.environment().functions()[index],
                path,
            })
            .collect::<Vec<_>>();
        let issues = match failure {
            Failure::Lock(error) => {
                let Some(Classified::Lock { findings, .. }) =
                    supplied.get(&std::ptr::from_ref(error))
                else {
                    return Err(Error::Opaque(error));
                };
                if findings.is_empty() {
                    return Err(invalid());
                }
                crate::project_lock_diagnostics::issues(run.captured_environment(), findings)
                    .map_err(|error| match error {
                        crate::project_lock_diagnostics::Error::Limit => {
                            Error::Report(report::Error::OutputLimit)
                        }
                        _ => invalid(),
                    })?
                    .into_iter()
                    .map(diagnostic)
                    .collect::<Result<Vec<_>, _>>()?
            }
            Failure::Bindings(failures) => {
                let details = failures
                    .iter()
                    .map(|failure| host(&supplied, &failure.error))
                    .collect::<Vec<_>>();
                let issues = activation::binding_issues(&definitions, failures, &details)
                    .map_err(project_error)?;
                activation_origins(
                    run,
                    &paths,
                    issues,
                    failures.iter().map(|failure| failure.function),
                )?
            }
            Failure::Tests(failures) => {
                let details = failures
                    .iter()
                    .map(|failure| match failure {
                        TestFailure::Invocation { error, .. } => {
                            invocation_host(&supplied, &error.kind)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let issues = activation::test_issues(&definitions, failures, &details)
                    .map_err(project_error)?;
                activation_origins(
                    run,
                    &paths,
                    issues,
                    failures.iter().map(|failure| match failure {
                        TestFailure::Invocation { function, .. }
                        | TestFailure::Result { function, .. } => *function,
                    }),
                )?
            }
            _ => return Err(Error::Boundary(boundary)),
        };
        diagnostics.extend(issues);
        // Before activation succeeds, the engine has no inspection/capture authority.
        if !attempt.dataset.sources.is_empty()
            || !matches!(attempt.dataset.result, Err(PortError::Incomplete))
        {
            return Err(invalid());
        }
        result = report::envelope_sources(run, &attempt.dataset.sources, &id)?;
    } else {
        result = report::envelope_sources(run, &attempt.dataset.sources, &id)?;
        match &attempt.dataset.result {
            Ok(dataset) => {
                let (checks, findings) =
                    checked_records(run, &dataset.retained_verifications, "verification", &id)?;
                verifications.extend(checks);
                diagnostics.extend(findings);
                match &dataset.result {
                    Ok(success) => {
                        let (checks, findings) =
                            checked_records(run, &success.verifications, "verification", &id)?;
                        if !findings.is_empty() {
                            return Err(invalid());
                        }
                        verifications.extend(checks);
                    }
                    Err(error) => match error.as_ref() {
                        ExecutionError::ProjectFunction {
                            path,
                            error,
                            identity,
                        } => diagnostics.push(diagnostic(
                            invocation::invocation(
                                path,
                                error,
                                identity.as_ref(),
                                &run.compiled().key_names().collect::<Vec<_>>(),
                                invocation_host(&supplied, &error.kind),
                            )
                            .map_err(project_error)?,
                        )?),
                        ExecutionError::KeyFailures(records)
                        | ExecutionError::VerificationFailures(records) => {
                            let phase = if matches!(error.as_ref(), ExecutionError::KeyFailures(_))
                            {
                                "output"
                            } else {
                                "verification"
                            };
                            let (checks, findings) = checked_records(run, records, phase, &id)?;
                            verifications.extend(checks);
                            diagnostics.extend(findings);
                        }
                        other => {
                            let completed = match other {
                                ExecutionError::VerificationDiagnostic { records, .. }
                                | ExecutionError::VerificationDeclaration { records, .. }
                                | ExecutionError::VerificationPredicate { records, .. } => {
                                    records.as_slice()
                                }
                                _ => &[],
                            };
                            let (checks, findings) =
                                checked_records(run, completed, "verification", &id)?;
                            verifications.extend(checks);
                            diagnostics.extend(findings);
                            let outcome = crate::dataset_transport::semantic_condition(other)
                                .map_err(|_| Error::Execution(other))?;
                            if outcome["status"] != "condition" {
                                return Err(Error::Execution(other));
                            }
                            if outcome["verifications"].is_array() {
                                let (checks, findings) =
                                    report::check_observations(run, &outcome, &id)?;
                                verifications.extend(checks);
                                diagnostics.extend(findings);
                            }
                            diagnostics.push(report::condition(run, &outcome)?);
                        }
                    },
                }
                report::set_handler_counts(
                    &mut result,
                    &json!({"handler_counts":dataset.handler_counts.iter().map(|count| json!({"spec_path":count.spec_path,"handler":count.handler.name(),"count":count.count.to_string()})).collect::<Vec<_>>()}),
                )?;
            }
            Err(error) => {
                let finding = |source: &yamaa_core::specification::SourceDeclaration,
                               failure: yamaa_core::resource::ResourceFailure|
                 -> Result<Value, report::Error> {
                    let written = run
                        .document()
                        .written_source_path(&source.name)
                        .ok_or(report::Error::InvalidObservation)?;
                    crate::specification_diagnostics::portable_diagnostic(
                        failure.diagnostic(&source.name, written),
                    )
                    .ok_or(report::Error::InvalidObservation)
                };
                match error {
                    PortError::Capture(_) => {
                        let Some((last, prefix)) = attempt.dataset.sources.split_last() else {
                            return Err(invalid());
                        };
                        if last.read.captured
                            || last.snapshot.is_some()
                            || last.table.is_some()
                            || prefix.iter().any(|source| {
                                !source.read.captured || source.read.failure.is_some()
                            })
                        {
                            return Err(invalid());
                        }
                        diagnostics.push(finding(
                            &last.read.source,
                            last.read.failure.ok_or(Error::Source(error))?,
                        )?);
                    }
                    PortError::Inspect(failures) => {
                        if !attempt.dataset.sources.is_empty() || failures.is_empty() {
                            return Err(invalid());
                        }
                        let mut sources = run.compiled().sources().iter();
                        for failure in failures {
                            if !sources.any(|source| source == &failure.source) {
                                return Err(invalid());
                            }
                            diagnostics.push(finding(
                                &failure.source,
                                failure.failure.ok_or(Error::Source(error))?,
                            )?);
                        }
                    }
                    PortError::Run(failure) => {
                        let findings = match failure {
                            RunError::Decode(error) => crate::specification_diagnostics::findings(
                                error,
                                Some(run.compiled().source()),
                            ),
                            RunError::Bind(error) => crate::specification_diagnostics::binding(
                                error,
                                run.compiled().source(),
                            ),
                            RunError::Sources(errors) => {
                                let mut findings = Vec::new();
                                for (index, ingestion) in errors {
                                    let source =
                                        run.compiled().sources().get(*index).ok_or_else(invalid)?;
                                    findings.extend(
                                        crate::specification_diagnostics::findings(
                                            ingestion,
                                            Some(source),
                                        )
                                        .ok_or(Error::Source(error))?,
                                    );
                                }
                                Some(findings)
                            }
                            _ => None,
                        }
                        .ok_or(Error::Source(error))?;
                        diagnostics.extend(findings);
                    }
                    _ => return Err(Error::Source(error)),
                }
            }
        }
    }
    report::set_diagnostics(&mut result, diagnostics);
    result["verifications"] = json!(verifications);
    result["activation"] = activation;
    Ok(report::prepare_result(
        run,
        execution,
        id,
        report::bounded(result)?,
    )?)
}
