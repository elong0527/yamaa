//! Borrowed ordinary host facts for one complete retained graph attempt.
//! All aggregate admission precedes diagnostic copies; opaque payloads stay held.
use crate::{
    producer_report::NativeAttempt,
    project_activation_diagnostics::{self as activation, DefinitionSource},
    project_activation_observations::ActivationView,
    project_function_diagnostics::{self as invocation, HostDetails},
    project_report::{self, Classified},
    specification_report::Error,
};
use serde_json::Value;
use std::collections::BTreeMap;
use yamaa_core::function_signature::ProjectFunctionIdentity;
use yamaa_core::{
    dataset::RowIdentity,
    function_invocation::{FailureKind, InvocationFailure},
    producer_build::Failure as BuildFailure,
    project_activation::{Failure, TestFailure},
};

pub(crate) struct Facts<'a, 'f, E>(BTreeMap<*const E, &'f Classified<'a, E>>);

impl<'a, 'f, E> Facts<'a, 'f, E> {
    pub(crate) fn admit(
        attempt: &'a NativeAttempt<E>,
        facts: &'f [Classified<'a, E>],
    ) -> Result<Self, Error> {
        if std::mem::size_of::<E>() == 0 && !facts.is_empty() {
            return Err(Error::InvalidObservation);
        }

        if facts.is_empty() {
            return Ok(Self(BTreeMap::new()));
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
    ) -> Result<Option<Value>, Error> {
        let plan = view.compiled();

        let declaring = view.document().declaring_source(path);
        for _ in 0..4 {
            if let Some(source) = declaring {}
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
            Err(invocation::Error::InvalidContext) => Err(Error::InvalidObservation),
        }
    }
    pub(crate) fn activation(
        &self,
        attempt: &'a NativeAttempt<E>,
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
                for _ in 0..4 {
                    if let Some(lock) = captured.lock() {}
                }
            }
            // The qualified lock projector admits the held authored lock spelling
            // and all typed facts before ownership. Its ceiling is cumulative here.
            return match crate::project_lock_diagnostics::issues(captured, findings) {
                Ok(issues) => issues
                    .into_iter()
                    .map(project_report::diagnostic)
                    .collect::<Result<_, _>>()
                    .map(Some),
                Err(_) => Err(Error::InvalidObservation),
            };
        }
        let count = match failure {
            Failure::Bindings(failures) => failures.len(),
            Failure::Tests(failures) => failures.len(),
            _ => return Ok(None),
        };
        let selected = held.called_functions();

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
        }

        match failure {
            Failure::Bindings(failures) => {
                for failure in failures {
                    let definition = selected
                        .get(failure.function)
                        .and_then(|&index| held.metadata().environment().functions().get(index))
                        .ok_or(Error::InvalidObservation)?
                        .definition();
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

                    match failure {
                        TestFailure::Invocation { error, .. } => {
                            invocation_budget("functions.tests", error, None, std::iter::empty)?
                        }
                        TestFailure::Result {
                            actual, expected, ..
                        } => {}
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
            Err(invocation::Error::InvalidContext) => Err(Error::InvalidObservation),
        }
    }
}
