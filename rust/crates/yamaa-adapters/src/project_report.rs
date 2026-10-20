//! Complete project reports borrow one owned attempt. Only explicit ordinary
//! host facts are portable; original opaque payloads and interrupts stay held.
use crate::{
    project_activation_diagnostics::{self as activation, DefinitionSource},
    project_function_diagnostics::{self as invocation, HostDetails},
    project_run::{Attempt, BoundaryFailure, PreparedRun},
    specification_report::{self as report, BuildResult, Identity},
};
use yamaa_core::application_issue::{Fault, Stage};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::json::Value;
use yamaa_core::{
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
/// Classify visited host failures for report formatting. Every host applies
/// the same rule: a lock failure is reported with its findings; otherwise the
/// failure is reported with its projected host details; failures with no
/// projection are skipped. Hosts supply only the lock findings and the host
/// details, both of which are host-specific (a host projects its own
/// exceptions/conditions through [`crate::callback::details`]); `details`
/// aligns with `failures` by index. A lock failure takes the lock branch even
/// when a details projection is also present.
pub fn classify_host_failures<'a, E>(
    failures: &[&'a E],
    details: &[Option<HostDetails<'a>>],
    lock_findings: impl Fn(&'a E) -> Option<&'a [crate::project_lock::Finding]>,
) -> Vec<Classified<'a, E>> {
    failures
        .iter()
        .zip(details)
        .filter_map(|(&failure, details)| {
            if let Some(findings) = lock_findings(failure) {
                return Some(Classified::Lock { failure, findings });
            }
            details.map(|details| Classified::Host { failure, details })
        })
        .collect()
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
impl<C, E> Error<'_, C, E> {
    /// The public refusal for a project result this attempt could not build.
    pub fn refusal(&self) -> crate::file_application::BuildRefusal {
        let (code, limit) = match self {
            Self::Opaque(_) => (Fault::OpaqueHost, None),
            Self::Boundary(_) => (Fault::ActivationBoundary, None),
            Self::Source(_) => (Fault::SourceBoundary, None),
            Self::Execution(_) => (Fault::ExecutionBoundary, None),
            Self::Report(report::Error::ResourceLimit(limit)) => {
                (Fault::OutputLimit, Some(limit.clone()))
            }
            Self::Report(_) => (Fault::ReportRefusal, None),
        };
        crate::file_application::BuildRefusal { code, limit }
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
fn project_error<C, E>(error: invocation::Error<'_, E>) -> Error<'_, C, E> {
    match error {
        invocation::Error::Opaque(error) => Error::Opaque(error),
        invocation::Error::InvalidContext => invalid(),
    }
}
pub(crate) fn checked_records(
    run: &dyn crate::specification_run_view::RunView,
    records: &[CheckRecord],
    phase: &str,
    id: &Identity<'_>,
) -> Result<(Vec<Value>, Vec<Diagnostic>), report::Error> {
    crate::project_codelist_diagnostics::codelist_issues(run.compiled(), records)
        .map_err(|_| report::Error::InvalidObservation)?;
    let phase = match phase {
        "output" => yamaa_core::verification_diagnostics::Phase::Output,
        "verification" => yamaa_core::verification_diagnostics::Phase::Verification,
        _ => return Err(report::Error::UnsupportedOutcome),
    };
    report::check_observations(run, records, phase, id)
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
    observed: &yamaa_core::project_activation::Observations,
    boundary: ActivationBoundary<'_, E>,
) -> Result<(), report::Error> {
    use yamaa_core::project_activation::{BindingOutcome, LockObservation, TestOutcome};
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
    let lock_ready = if run.environment().lock().is_some() {
        observed.lock == LockObservation::Verified
    } else {
        observed.lock == LockObservation::NotRequested
    };
    let bound = lock_ready
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
                || !lock_ready
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
    issues: Vec<Diagnostic>,
    functions: impl Iterator<Item = usize> + Clone,
) -> Result<Vec<Diagnostic>, report::Error> {
    let captured = run.captured_environment();

    let functions = functions.collect::<Vec<_>>();
    if issues.len() != functions.len() {
        return Err(report::Error::InvalidObservation);
    }
    let entry = captured.root().source().identity.as_str();
    // Inline functions are declared by the linked environment file.
    let declaring = captured
        .environment_file()
        .map_or(entry, |file| file.source().identity.as_str());
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
        let source = external.unwrap_or(declaring);
        metadata.push((source, declaration, external.is_some()));
    }
    issues
        .into_iter()
        .zip(metadata)
        .map(|(mut finding, (source, declaration, external))| {
            if external {
                for path in &mut finding.spec_paths {
                    let suffix = path
                        .strip_prefix(declaration.as_str())
                        .and_then(|suffix| suffix.strip_prefix('.'))
                        .ok_or(report::Error::InvalidObservation)?;
                    *path = suffix.to_owned();
                }
            }
            for (key, value) in [
                ("source", source),
                ("entry", entry),
                ("environment_path", declaration.as_str()),
            ] {
                finding
                    .context
                    .insert(key.into(), Value::String(value.into()));
            }
            Ok(finding)
        })
        .collect()
}

/// Build the report envelope for an activation-boundary failure, appending
/// its diagnostics to the caller's collection in the original order.
fn activation_failure_result<'a, C, E>(
    run: &PreparedRun,
    attempt: &'a Attempt<C, E>,
    boundary: &'a BoundaryFailure<E>,
    supplied: &FactMap<'a, '_, E>,
    id: &Identity<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<crate::report_storage::StoredReport, Error<'a, C, E>> {
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
            let Some(Classified::Lock { findings, .. }) = supplied.get(&std::ptr::from_ref(error))
            else {
                return Err(Error::Opaque(error));
            };
            if findings.is_empty() {
                return Err(invalid());
            }
            crate::project_lock_diagnostics::issues(run.captured_environment(), findings)
                .map_err(|_| invalid())?
        }
        Failure::Bindings(failures) => {
            let details = failures
                .iter()
                .map(|failure| host(supplied, &failure.error))
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
                    TestFailure::Invocation { error, .. } => invocation_host(supplied, &error.kind),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let issues =
                activation::test_issues(&definitions, failures, &details).map_err(project_error)?;
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
    Ok(report::envelope_sources(
        run,
        &attempt.dataset.sources,
        id,
        true,
    )?)
}

/// Neither reporting nor obtaining output invokes a port. Publication remains
/// the same explicit successful-output gate used by ordinary specification runs.
pub fn build_result<'a, C, E>(
    run: &PreparedRun,
    attempt: &'a Attempt<C, E>,
    id: Identity<'_>,
    facts: &[Classified<'a, E>],
) -> Result<BuildResult, Error<'a, C, E>> {
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
        .map_err(|_| invalid())?;
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
        result =
            activation_failure_result(run, attempt, boundary, &supplied, &id, &mut diagnostics)?;
    } else {
        result = report::envelope_sources(run, &attempt.dataset.sources, &id, true)?;
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
                        } => diagnostics.push(
                            invocation::invocation(
                                path,
                                error,
                                identity.as_ref(),
                                &run.compiled().key_names().collect::<Vec<_>>(),
                                invocation_host(&supplied, &error.kind),
                            )
                            .map_err(project_error)?,
                        ),
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
                            if let Some(refusal) = crate::resource_diagnostics::execution(
                                other,
                                run.compiled().sources(),
                                run.compiled().driver_index(),
                            ) {
                                diagnostics.push(
                                    refusal.diagnostic(Stage::Execute, Fault::ExecutionLimit),
                                );
                            } else {
                                let crate::dataset_result::Outcome::Condition(found) =
                                    crate::dataset_result::semantic_condition(other)
                                        .map_err(|_| Error::Execution(other))?
                                else {
                                    return Err(Error::Execution(other));
                                };
                                diagnostics.push(report::condition(run, &found)?);
                            }
                        }
                    },
                }
                report::set_handler_counts(&mut result, &dataset.handler_counts);
            }
            Err(error) => {
                let finding = |source: &yamaa_core::specification::SourceDeclaration,
                               failure: yamaa_core::resource::ResourceFailure|
                 -> Result<Diagnostic, report::Error> {
                    let written = run
                        .document()
                        .written_source_path(&source.name)
                        .ok_or(report::Error::InvalidObservation)?;
                    Ok(failure.diagnostic(&source.name, written))
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
                            RunError::Decode(error) => {
                                crate::resource_diagnostics::source_findings(
                                    error,
                                    run.compiled().source(),
                                )
                                .map_err(report::Error::from)?
                            }
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
                                        crate::resource_diagnostics::source_findings(
                                            ingestion, source,
                                        )
                                        .map_err(report::Error::from)?
                                        .ok_or(Error::Source(error))?,
                                    );
                                }
                                Some(findings)
                            }
                            RunError::AdmittedSources(errors) => {
                                crate::source_diagnostics::admitted_findings(
                                    errors,
                                    run.compiled().sources(),
                                )
                                .map_err(report::Error::from)?
                            }
                            _ => crate::resource_diagnostics::run_limit(
                                failure,
                                run.compiled().sources(),
                            )
                            .map(|refusal| {
                                vec![refusal.diagnostic(Stage::Ingest, Fault::SourceLimit)]
                            }),
                        }
                        .ok_or(Error::Source(error))?;
                        diagnostics.extend(findings);
                    }
                    _ => return Err(Error::Source(error)),
                }
            }
        }
    }
    result.set_diagnostics(diagnostics);
    result["verifications"] = Value::Array(verifications);
    result["activation"] = activation;
    let facts = crate::run_report::RunFacts::captured(
        &attempt.dataset.sources,
        attempt
            .dataset
            .result
            .as_ref()
            .map_or(&[], |dataset| dataset.template_rows.as_slice()),
    );
    Ok(report::prepare_result(run, execution, id, result, &facts)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use yamaa_core::project_diagnostics::HostDetails;

    enum Failure {
        Lock,
        Host,
        Silent,
    }

    #[test]
    fn classify_host_failures_prefers_lock_then_details_then_skip() {
        let findings = [crate::project_lock::Finding {
            package: "pkg".into(),
            reason: yamaa_core::diagnostic::LockReason::VersionMismatch,
            expected: Vec::new(),
            actual: None,
        }];
        let lock = Failure::Lock;
        let host = Failure::Host;
        let silent = Failure::Silent;
        let failures: Vec<&Failure> = vec![&lock, &host, &silent];
        // The lock failure also carries a details projection; the lock
        // branch must win. The silent failure has no projection and is
        // skipped.
        let details = vec![
            Some(HostDetails::Rejected {
                reason: "lock projection must not win",
                returned: None,
            }),
            Some(HostDetails::Exception {
                class: "ValueError",
                message: "boom",
            }),
            None,
        ];
        let classified = classify_host_failures(&failures, &details, |failure| match failure {
            Failure::Lock => Some(findings.as_slice()),
            _ => None,
        });
        assert_eq!(classified.len(), 2);
        assert!(matches!(classified[0], Classified::Lock { .. }));
        assert!(matches!(classified[1], Classified::Host { .. }));
        assert!(std::ptr::eq(classified[0].failure(), &lock));
        assert!(std::ptr::eq(classified[1].failure(), &host));
    }
}
