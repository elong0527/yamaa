//! Borrowed ordinary host facts for one complete retained graph attempt.
//! All aggregate admission precedes diagnostic copies; opaque payloads stay held.
use crate::{
    producer_report::NativeAttempt,
    project_activation_diagnostics::{self as activation, DefinitionSource},
    project_activation_observations::ActivationView,
    project_function_diagnostics::{self as invocation, HostDetails},
    project_report::{self, Classified},
    report_projection::Budget,
    specification_report::Error,
};
use serde_json::Value;
use std::collections::BTreeMap;
use yamaa_core::function_signature::ProjectFunctionIdentity;
use yamaa_engine::{
    dataset::RowIdentity,
    function_invocation::{FailureKind, InvocationFailure},
    producer_build::Failure as BuildFailure,
    project_activation::{Failure, TestFailure},
};

pub(crate) struct Facts<'a, 'f, E>(BTreeMap<*const E, &'f Classified<'a, E>>);

fn host_budget(budget: &mut Budget, details: HostDetails<'_>) -> Result<(), Error> {
    let fields = match details {
        HostDetails::Exception { class, message, .. } => [Some(class), Some(message)],
        HostDetails::Rejected { reason, returned } => [Some(reason), returned],
    };
    for text in fields.into_iter().flatten() {
        if text.len() > crate::function_transport::MAX_ERROR_BYTES {
            return Err(Error::OutputLimit);
        }
        // A conformance issue nests invocation facts within a second context.
        for _ in 0..8 {
            budget.text(text)?;
        }
    }
    Ok(())
}
impl<'a, 'f, E> Facts<'a, 'f, E> {
    pub(crate) fn admit(
        attempt: &'a NativeAttempt<E>,
        facts: &'f [Classified<'a, E>],
        budget: &mut Budget,
    ) -> Result<Self, Error> {
        if std::mem::size_of::<E>() == 0 && !facts.is_empty() {
            return Err(Error::InvalidObservation);
        }
        budget.entries(facts.len())?;
        if facts.is_empty() {
            return Ok(Self(BTreeMap::new()));
        }
        budget.work(attempt.graph.nodes.len())?;
        if let Err(BuildFailure::Activation(failure)) = &attempt.graph.outcome {
            budget.work(match failure {
                Failure::Bindings(failures) => failures.len(),
                Failure::Tests(failures) => failures.len(),
                _ => 1,
            })?;
        }
        let mut count = 0usize;
        crate::producer_attempt::visit_host_failures(attempt, |_, _| {
            count = count.saturating_add(1);
        });
        budget.entries(count)?;
        // Admit every borrowed string/count before even the fact maps grow.
        for fact in facts {
            match fact {
                Classified::Host { details, .. } => host_budget(budget, *details)?,
                Classified::Lock { findings, .. } => {
                    budget.entries(findings.len())?;
                    for finding in *findings {
                        budget.entries(32)?;
                        budget.entries(finding.expected.len())?;
                        for _ in 0..4 {
                            for text in std::iter::once(finding.package.as_str())
                                .chain(finding.expected.iter().map(String::as_str))
                                .chain(finding.actual.as_deref())
                            {
                                budget.text(text)?;
                            }
                        }
                    }
                }
            }
        }
        let mut held = BTreeMap::new();
        crate::producer_attempt::visit_host_failures(attempt, |stage, error| {
            held.insert(std::ptr::from_ref(error), stage == "lock");
        });
        let mut supplied = BTreeMap::new();
        for fact in facts {
            let pointer = std::ptr::from_ref(fact.failure());
            let is_lock = held.get(&pointer).ok_or(Error::InvalidObservation)?;
            if *is_lock != matches!(fact, Classified::Lock { .. })
                || supplied.insert(pointer, fact).is_some()
            {
                return Err(Error::InvalidObservation);
            }
        }
        Ok(Self(supplied))
    }
    fn host(&self, error: &E) -> Option<HostDetails<'a>> {
        match self.0.get(&std::ptr::from_ref(error)) {
            Some(Classified::Host { details, .. }) => Some(*details),
            _ => None,
        }
    }
    fn invocation_host(&self, failure: &FailureKind<E>) -> Option<HostDetails<'a>> {
        match failure {
            FailureKind::CallFailed(error) | FailureKind::InvalidHostResult(error) => {
                self.host(error)
            }
            _ => None,
        }
    }
    pub(crate) fn invocation(
        &self,
        path: &str,
        failure: &'a InvocationFailure<E, ProjectFunctionIdentity>,
        row: Option<&RowIdentity>,
        view: &crate::file_producer_build::NodeReport<'_>,
        entry: &str,
        budget: &mut Budget,
    ) -> Result<Option<Value>, Error> {
        let plan = view.compiled();
        invocation_budget(path, failure, row, || plan.key_names(), budget)?;
        budget.work(view.document().inheritance().map_or(0, |inherited| {
            inherited
                .provenance()
                .len()
                .saturating_mul(path.len().saturating_add(1))
        }))?;
        let declaring = view.document().declaring_source(path);
        for _ in 0..4 {
            budget.text(&view.document().source().identity)?;
            budget.text(entry)?;
            if let Some(source) = declaring {
                budget.text(source)?;
            }
        }
        let keys = plan.key_names().collect::<Vec<_>>();
        match invocation::invocation(
            path,
            failure,
            row,
            &keys,
            self.invocation_host(&failure.kind),
        ) {
            Ok(issue) => {
                let mut value = project_report::diagnostic(issue)?;
                let context = value["context"]
                    .as_object_mut()
                    .ok_or(Error::InvalidObservation)?;
                context.insert(
                    "source".into(),
                    serde_json::json!(view.document().source().identity),
                );
                context.insert("entry".into(), serde_json::json!(entry));
                context.insert("declaring_sources".into(), serde_json::json!([declaring]));
                Ok(Some(value))
            }
            Err(invocation::Error::Opaque(_)) => Ok(None),
            Err(invocation::Error::Limit) => Err(Error::OutputLimit),
            Err(invocation::Error::InvalidContext) => Err(Error::InvalidObservation),
        }
    }
    pub(crate) fn activation(
        &self,
        attempt: &'a NativeAttempt<E>,
        budget: &mut Budget,
    ) -> Result<Option<Vec<Value>>, Error> {
        let Err(BuildFailure::Activation(failure)) = &attempt.graph.outcome else {
            return Ok(Some(Vec::new()));
        };
        let held = attempt.provenance.as_ref();
        let captured = held.captured_environment();
        if let Failure::Lock(error) = failure {
            let Some(Classified::Lock { findings, .. }) = self.0.get(&std::ptr::from_ref(error))
            else {
                return Ok(None);
            };
            if findings.is_empty() {
                return Err(Error::InvalidObservation);
            }
            budget.work(captured.root().raw().document.nodes().len())?;
            let raw = &captured.root().raw().document;
            let written = raw
                .field(raw.root(), "lock")
                .and_then(|index| raw.nodes().get(index))
                .and_then(|node| match node {
                    yamaa_core::schema::DocumentNode::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .ok_or(Error::InvalidObservation)?;
            for _ in 0..findings.len() {
                budget.entries(32)?;
                for _ in 0..4 {
                    budget.text(&captured.root().source().identity)?;
                    budget.text(written)?;
                    if let Some(lock) = captured.lock() {
                        budget.text(&lock.source.identity)?;
                    }
                }
            }
            // The qualified lock projector admits the held authored lock spelling
            // and all typed facts before ownership. Its ceiling is cumulative here.
            return match crate::project_lock_diagnostics::issues_with_limit(
                captured,
                findings,
                budget.remaining_bytes(),
            ) {
                Ok(issues) => issues
                    .into_iter()
                    .map(project_report::diagnostic)
                    .collect::<Result<_, _>>()
                    .map(Some),
                Err(crate::project_lock_diagnostics::Error::Limit) => Err(Error::OutputLimit),
                Err(_) => Err(Error::InvalidObservation),
            };
        }
        let count = match failure {
            Failure::Bindings(failures) => failures.len(),
            Failure::Tests(failures) => failures.len(),
            _ => return Ok(None),
        };
        let selected = held.called_functions();
        budget.entries(selected.len())?;
        budget.entries(count)?;
        budget.work(
            count
                .checked_mul(captured.captures().len())
                .ok_or(Error::OutputLimit)?,
        )?;
        // Preflight the whole selected declaration set and all failure contexts,
        // including external authored origins, before formatting paths or issues.
        for &index in selected {
            let definition = held
                .metadata()
                .environment()
                .functions()
                .get(index)
                .ok_or(Error::InvalidObservation)?
                .definition();
            for _ in 0..4 {
                budget.text("functions.")?;
                budget.text(&definition.name)?;
                budget.text(&definition.function)?;
            }
        }
        for _ in 0..count {
            budget.entries(64)?;
            for _ in 0..4 {
                budget.text(&captured.root().source().identity)?;
                for capture in captured.captures() {
                    budget.text(&capture.document.source().identity)?;
                }
            }
        }
        match failure {
            Failure::Bindings(failures) => {
                for failure in failures {
                    let definition = selected
                        .get(failure.function)
                        .and_then(|&index| held.metadata().environment().functions().get(index))
                        .ok_or(Error::InvalidObservation)?
                        .definition();
                    for _ in 0..4 {
                        budget.text(&definition.name)?;
                        budget.text(&definition.function)?;
                    }
                }
            }
            Failure::Tests(failures) => {
                for failure in failures {
                    let (function, case) = match failure {
                        TestFailure::Invocation { function, case, .. }
                        | TestFailure::Result { function, case, .. } => (*function, *case),
                    };
                    let definition = selected
                        .get(function)
                        .and_then(|&index| held.metadata().environment().functions().get(index))
                        .ok_or(Error::InvalidObservation)?
                        .definition();
                    let test = definition
                        .tests
                        .get(case)
                        .ok_or(Error::InvalidObservation)?;
                    for _ in 0..8 {
                        for text in [&definition.name, &definition.function, &test.id] {
                            budget.text(text)?;
                        }
                        budget.entries(test.args.len())?;
                        for (name, value) in &test.args {
                            budget.text(name)?;
                            budget.scalar(value)?;
                        }
                        budget.scalar(&test.result)?;
                    }
                    match failure {
                        TestFailure::Invocation { error, .. } => invocation_budget(
                            "functions.tests",
                            error,
                            None,
                            std::iter::empty,
                            budget,
                        )?,
                        TestFailure::Result {
                            actual, expected, ..
                        } => {
                            for _ in 0..4 {
                                budget.scalar(actual)?;
                                budget.scalar(expected)?;
                            }
                        }
                    }
                }
            }
            _ => unreachable!(),
        }
        let paths = selected
            .iter()
            .map(|&index| {
                format!(
                    "functions.{}",
                    held.metadata().environment().functions()[index]
                        .definition()
                        .name
                )
            })
            .collect::<Vec<_>>();
        let definitions = selected
            .iter()
            .zip(&paths)
            .map(|(&index, path)| DefinitionSource {
                function: &held.metadata().environment().functions()[index],
                path,
            })
            .collect::<Vec<_>>();
        let result = match failure {
            Failure::Bindings(failures) => {
                let details = failures
                    .iter()
                    .map(|failure| self.host(&failure.error))
                    .collect::<Vec<_>>();
                activation::binding_issues(&definitions, failures, &details).map(|issues| {
                    project_report::activation_origins(
                        held,
                        &paths,
                        issues,
                        failures.iter().map(|failure| failure.function),
                    )
                })
            }
            Failure::Tests(failures) => {
                let details = failures
                    .iter()
                    .map(|failure| match failure {
                        TestFailure::Invocation { error, .. } => self.invocation_host(&error.kind),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                activation::test_issues(&definitions, failures, &details).map(|issues| {
                    project_report::activation_origins(
                        held,
                        &paths,
                        issues,
                        failures.iter().map(|failure| match failure {
                            TestFailure::Invocation { function, .. }
                            | TestFailure::Result { function, .. } => *function,
                        }),
                    )
                })
            }
            _ => unreachable!(),
        };
        match result {
            Ok(issues) => issues.map(Some),
            Err(invocation::Error::Opaque(_)) => Ok(None),
            Err(invocation::Error::Limit) => Err(Error::OutputLimit),
            Err(invocation::Error::InvalidContext) => Err(Error::InvalidObservation),
        }
    }
}

fn invocation_budget<'k, E, I: Iterator<Item = &'k str>>(
    path: &str,
    failure: &InvocationFailure<E, ProjectFunctionIdentity>,
    row: Option<&RowIdentity>,
    keys: impl Fn() -> I,
    budget: &mut Budget,
) -> Result<(), Error> {
    budget.entries(64)?;
    for _ in 0..4 {
        for text in [path, &failure.identity.name, &failure.identity.call] {
            budget.text(text)?;
        }
        for key in keys() {
            budget.entries(1)?;
            budget.text(key)?;
        }
        if let Some(row) = row {
            budget.entries(row.values.len())?;
            for value in &row.values {
                budget.scalar(value)?;
            }
        }
        match &failure.kind {
            FailureKind::UnknownArguments(names) => {
                budget.entries(names.len())?;
                for name in names {
                    budget.text(name)?;
                }
            }
            FailureKind::MissingRequired { parameter }
            | FailureKind::ArgumentType { parameter, .. } => budget.text(parameter)?,
            _ => {}
        }
    }
    Ok(())
}
