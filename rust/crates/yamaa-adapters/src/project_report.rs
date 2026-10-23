//! The project half of one attempt's report: its activation evidence, its
//! activation failures and its project-function invocations. Only explicit
//! ordinary host facts are portable; original opaque payloads and interrupts
//! stay held.
use crate::{
    project_activation_diagnostics::{self as activation, DefinitionSource},
    project_function_diagnostics::{self as invocation, HostDetails},
    specification_report::{self as report, Identity, Refusal},
    specification_run::{Attempt, BoundaryFailure, PreparedRun},
};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::json::Value;
use yamaa_core::{
    dataset::CheckRecord,
    function_invocation::FailureKind,
    project_activation::{Failure, TestFailure},
    specification_run::PortError,
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

fn invalid<'a, E>() -> Refusal<'a, E> {
    Refusal::Report(report::Error::InvalidObservation)
}
pub(crate) type FactMap<'a, 'f, E> = std::collections::BTreeMap<*const E, &'f Classified<'a, E>>;
fn host<'a, E>(facts: &FactMap<'a, '_, E>, error: &E) -> Option<HostDetails<'a>> {
    match facts.get(&std::ptr::from_ref(error)) {
        Some(Classified::Host { details, .. }) => Some(*details),
        _ => None,
    }
}
pub(crate) fn invocation_host<'a, E>(
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
pub(crate) fn project_error<E>(error: invocation::Error<'_, E>) -> Refusal<'_, E> {
    match error {
        invocation::Error::Opaque(error) => Refusal::Opaque(error),
        invocation::Error::InvalidContext => invalid(),
    }
}
pub(crate) fn checked_records(
    run: &PreparedRun,
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
    run: &PreparedRun,
    observed: &yamaa_core::project_activation::Observations,
    boundary: ActivationBoundary<'_, E>,
) -> Result<(), report::Error> {
    use yamaa_core::project_activation::{BindingOutcome, LockObservation, TestOutcome};
    let selected = run.compiled().called_functions();
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

/// Place each activation finding in the function file that holds its
/// definition alone, so a finding's path starts at that file's root. Each
/// finding names that file as `source`, the project file as `entry` and the
/// project's `functions[i]` that lists it as `environment_path` (FUNC-2, D8).
pub(crate) fn activation_origins(
    run: &PreparedRun,
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
    let mut metadata = Vec::new();
    for &selected in &functions {
        let &index = run
            .compiled()
            .called_functions()
            .get(selected)
            .ok_or(report::Error::InvalidObservation)?;
        let name = run
            .environment()
            .functions()
            .get(index)
            .ok_or(report::Error::InvalidObservation)?
            .name();
        if captured.origins().functions.get(index).map(String::as_str) != Some(name) {
            return Err(report::Error::InvalidObservation);
        }
        let declaration = paths
            .get(selected)
            .ok_or(report::Error::InvalidObservation)?;
        let origin = captured
            .function_origin(index)
            .filter(|origin| origin.declaration() == *declaration)
            .ok_or(report::Error::InvalidObservation)?;
        metadata.push((origin.source, declaration));
    }
    issues
        .into_iter()
        .zip(metadata)
        .map(|(mut finding, (source, declaration))| {
            for path in &mut finding.spec_paths {
                let suffix = path
                    .strip_prefix(declaration.as_str())
                    .and_then(|suffix| suffix.strip_prefix('.'))
                    .ok_or(report::Error::InvalidObservation)?;
                *path = suffix.to_owned();
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

/// Each called function's definition beside its declaration in `paths`.
fn definition_sources<'p>(run: &'p PreparedRun, paths: &'p [String]) -> Vec<DefinitionSource<'p>> {
    run.compiled()
        .called_functions()
        .iter()
        .zip(paths)
        .map(|(&index, path)| DefinitionSource {
            function: &run.environment().functions()[index],
            path,
        })
        .collect()
}

/// The report of an activation-boundary failure: its findings, in the
/// original order, over an attempt that captured no source.
pub(crate) fn activation_failure<'a, C, E>(
    run: &PreparedRun,
    attempt: &'a Attempt<C, E>,
    boundary: &'a BoundaryFailure<E>,
    supplied: &FactMap<'a, '_, E>,
    id: &Identity<'_>,
) -> Result<crate::report_storage::StoredReport, Refusal<'a, E>> {
    let BoundaryFailure::Activation(failure) = boundary else {
        return Err(Refusal::Boundary(boundary));
    };
    // Each called function's declaration, `functions[i]` in the project
    // file, which its findings' paths start with.
    let paths = run
        .compiled()
        .called_functions()
        .iter()
        .map(|&index| {
            run.captured_environment()
                .function_origin(index)
                .map(|origin| origin.declaration())
        })
        .collect::<Option<Vec<_>>>();
    let issues = match failure {
        Failure::Lock(error) => {
            let Some(Classified::Lock { findings, .. }) = supplied.get(&std::ptr::from_ref(error))
            else {
                return Err(Refusal::Opaque(error));
            };
            if findings.is_empty() {
                return Err(invalid());
            }
            crate::project_lock_diagnostics::issues(
                run.captured_environment(),
                run.environment().host(),
                findings,
            )
            .map_err(|_| invalid())?
        }
        Failure::Bindings(failures) => {
            let paths = paths.ok_or_else(invalid)?;
            let definitions = definition_sources(run, &paths);
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
            let paths = paths.ok_or_else(invalid)?;
            let definitions = definition_sources(run, &paths);
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
        _ => return Err(Refusal::Boundary(boundary)),
    };
    // Before activation succeeds, the engine has no inspection/capture authority.
    if !attempt.dataset.sources.is_empty()
        || !matches!(attempt.dataset.result, Err(PortError::Incomplete))
    {
        return Err(invalid());
    }
    let mut report = report::envelope_sources(&attempt.dataset.sources, id, true)?;
    report.set_diagnostics(issues);
    report["verifications"] = Value::Array(Vec::new());
    Ok(report)
}

/// Index the classification facts a host supplied for this attempt's held
/// failures. Facts refer to the exact payload in this attempt, never to an
/// error string or an ordinal supplied by a host.
pub(crate) fn supplied<'a, 'f, C, E>(
    attempt: &'a Attempt<C, E>,
    facts: &'f [Classified<'a, E>],
) -> Result<FactMap<'a, 'f, E>, Refusal<'a, E>> {
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
        // Lock findings belong to the lock failure alone. A host fact about
        // the lock failure carries no lock findings, so that failure stays
        // opaque.
        let lock = matches!(&attempt.boundary, Err(BoundaryFailure::Activation(Failure::Lock(error))) if std::ptr::eq(fact.failure(), error));
        if !lock && matches!(fact, Classified::Lock { .. }) {
            return Err(invalid());
        }
    }
    Ok(supplied)
}

/// Validate the activation evidence of an attempt and render it.
pub(crate) fn activation_evidence<'a, C, E>(
    run: &PreparedRun,
    attempt: &'a Attempt<C, E>,
) -> Result<Value, Refusal<'a, E>> {
    let activation = crate::project_activation_observations::activation(run, &attempt.activation)
        .map_err(|_| invalid())?;
    let boundary = match &attempt.boundary {
        Ok(()) => ActivationBoundary::Complete,
        Err(BoundaryFailure::Activation(error)) => ActivationBoundary::Failed(error),
        _ => ActivationBoundary::Incomplete,
    };
    validate_activation(run, &attempt.activation, boundary)?;
    Ok(activation)
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
