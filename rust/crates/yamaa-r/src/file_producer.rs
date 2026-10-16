//! Actual producer results retain R callback payloads on the calling thread.
use extendr_api::prelude::*;
pub(crate) mod platform {
    use super::*;
    use crate::{project_activation::Error as HostError, project_result::platform::ordinary};
    use std::{cell::RefCell, rc::Rc};
    use yamaa_adapters::{
        callback::{details, CallbackError},
        file_application::BuildRefusal,
        file_producer_build::FileBuild,
        producer_result::RootResult,
        project_report::Classified,
    };
    type NativeAttempt = yamaa_adapters::file_producer_build::OwnedAttempt<HostError>;
    struct Attempt {
        inner: RefCell<NativeAttempt>,
        fields: Vec<String>,
    }
    pub(crate) struct PublicResult {
        attempt: Rc<Attempt>,
        observations: Option<String>,
        domain: Result<Box<RootResult>, BuildRefusal>,
        output_error: Option<yamaa_adapters::ipc_table::TableError>,
    }
    fn build_selected_attempt(
        prepared: &mut FileBuild,
        fields: Vec<String>,
        verify: Function,
        resolve_host: Function,
        scope: yamaa_core::application_observer::Scope<'_>,
    ) -> Rc<Attempt> {
        let provenance = std::sync::Arc::clone(prepared.provenance());
        let mut port = crate::project_activation::Port::new(
            provenance
                .environment()
                .lock()
                .map_or(&[], |lock| &lock.source.bytes),
            verify,
            resolve_host,
        );
        let inner = prepared.build_owned_reported(
            &mut port,
            crate::report_fields::identity(&fields),
            scope,
        );
        Rc::new(Attempt {
            inner: RefCell::new(inner),
            fields,
        })
    }

    pub(crate) fn build_selected(
        prepared: &mut FileBuild,
        fields: Vec<String>,
        verify: Function,
        resolve_host: Function,
        formatter: Function,
        scope: yamaa_core::application_observer::Scope<'_>,
    ) -> Result<PublicResult, String> {
        let attempt = build_selected_attempt(prepared, fields, verify, resolve_host, scope);
        scope.phase(yamaa_core::application_observer::Phase::Output);
        let converted = format_attempt(attempt, &formatter);
        scope.exit(
            yamaa_core::application_observer::Phase::Output,
            if converted.is_ok() {
                yamaa_core::application_observer::Outcome::Succeeded
            } else {
                yamaa_core::application_observer::Outcome::Failed
            },
        );
        converted
    }
    impl PublicResult {
        pub(crate) fn accepted(&self) -> bool {
            self.domain
                .as_ref()
                .is_ok_and(|result| result.output().is_some())
        }
        pub(crate) fn save(
            &mut self,
        ) -> Result<bool, yamaa_adapters::file_application::FailedBuild> {
            let result = self
                .domain
                .as_mut()
                .map_err(|_| yamaa_adapters::file_application::FailedBuild)?;
            let _owner = self
                .attempt
                .inner
                .try_borrow_mut()
                .map_err(|_| yamaa_adapters::file_application::FailedBuild)?;
            result.save()
        }
        pub(crate) fn output_table(&self) -> Option<&yamaa_adapters::public_table::PublicTable> {
            self.domain
                .as_ref()
                .ok()
                .and_then(|result| result.output_table())
        }
        pub(crate) fn log_table(
            &self,
            warning: bool,
        ) -> Option<&yamaa_adapters::public_table::PublicTable> {
            self.domain
                .as_ref()
                .ok()
                .and_then(|result| result.log_table(warning))
        }
        pub(crate) fn issues(&self) -> Vec<yamaa_adapters::issue_rows::Issue> {
            match &self.domain {
                Ok(result) => result.issues().to_vec(),
                Err(refusal) => refusal.issues(),
            }
        }
        pub(crate) fn observations(&self) -> Option<String> {
            match &self.domain {
                Ok(result) => Some(result.observations_json()),
                Err(_) => self.observations.clone(),
            }
        }
        pub(crate) fn projection_failure(&self) -> Option<String> {
            self.output_error.map(|error| error.to_string())
        }
        pub(crate) fn interrupt(&self) -> Result<Robj, String> {
            held_interrupt(&self.attempt)
        }
        pub(crate) fn evidence(&self) -> Result<Robj, String> {
            held_evidence(&self.attempt)
        }
    }
    fn format_attempt(attempt: Rc<Attempt>, formatter: &Function) -> Result<PublicResult, String> {
        // Exclusive borrow spans host classification. Reentrant reads or reports
        // refuse promptly; the original rooted condition cannot be replaced.
        let observations = attempt
            .inner
            .try_borrow_mut()
            .map_err(|_| "producer attempt already borrowed")?;
        let mut failures = Vec::new();
        yamaa_adapters::producer_attempt::visit_host_failures(&observations, |_, failure| {
            failures.push(failure)
        });
        let projections = failures
            .iter()
            .map(|failure| match failure {
                HostError::Host(condition) => ordinary(condition, formatter, false),
                HostError::Representation(condition) => ordinary(condition, formatter, true),
                HostError::Scalar(reason) => Some(CallbackError::Rejected {
                    reason: (*reason).into(),
                    returned: None,
                }),
                _ => None,
            })
            .collect::<Vec<_>>();
        let facts = failures
            .iter()
            .zip(&projections)
            .filter_map(|(&failure, projection)| match failure {
                HostError::Lock(findings) => Some(Classified::Lock { failure, findings }),
                _ => projection
                    .as_ref()
                    .and_then(details)
                    .map(|details| Classified::Host { failure, details }),
            })
            .collect::<Vec<_>>();
        let projection = yamaa_adapters::producer_result::project_owned(
            &observations,
            crate::report_fields::identity(&attempt.fields),
            &facts,
        );
        drop(observations);
        Ok(PublicResult {
            attempt,
            observations: projection.observations,
            domain: projection.result.map(Box::new),
            output_error: projection.output_error,
        })
    }
    fn held_evidence(attempt: &Attempt) -> Result<Robj, String> {
        let held = attempt
            .inner
            .try_borrow()
            .map_err(|_| "producer attempt already borrowed")?;
        let sources = &held.resources;
        let mut failures = Vec::new();
        // This traversal never renders a condition or calls a port.
        yamaa_adapters::producer_attempt::visit_host_failures(&held, |stage, failure| {
            failures.push(list!(stage = stage, facts = failure.facts()));
        });
        Ok(list!(
            accepted = held.accepted(),
            entered = List::from_values(held.graph.nodes.iter().map(|node| r!(node.node as f64))),
            failures = List::from_values(failures),
            sources = List::from_values(sources.iter().map(|(name, bytes)| list!(
                name = Raw::from_bytes(name.as_bytes()),
                bytes = Raw::from_bytes(bytes)
            )))
        )
        .into_robj())
    }
    fn held_interrupt(attempt: &Attempt) -> Result<Robj, String> {
        let held = attempt
            .inner
            .try_borrow()
            .map_err(|_| "producer attempt already borrowed")?;
        let mut interrupt = None;
        yamaa_adapters::producer_attempt::visit_host_failures(&held, |_, failure| {
            if let HostError::Interrupt(condition) = failure {
                if interrupt.is_none() {
                    interrupt = Some(condition.clone());
                }
            }
        });
        Ok(interrupt.unwrap_or_else(|| r!(NULL)))
    }
}
