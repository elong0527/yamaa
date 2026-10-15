//! Actual producer results retain R callback payloads on the calling thread.
use extendr_api::prelude::*;
pub(crate) mod platform {
    use super::*;
    use crate::project_activation::Error as HostError;
    use std::{cell::RefCell, rc::Rc};
    use yamaa_adapters::{
        callback::CallbackError,
        file_producer_build::{FileBuild, Limits},
        producer_report::{self, ProjectionError},
        project_function_diagnostics::HostDetails,
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
        refusal: &'static str,
        refusal_details: Option<yamaa_adapters::resource_diagnostics::Refusal>,
        domain: Option<Box<yamaa_adapters::producer_result::RootResult>>,
        output_error: Option<yamaa_adapters::ipc_table::TableError>,
    }
    fn build_selected_attempt_with_observer(
        prepared: &mut FileBuild,
        fields: Vec<String>,
        verify: Function,
        resolve_host: Function,
        maximum: usize,
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
        let mut limits = Limits::default();
        limits.engine.report_bytes = maximum;
        let inner = prepared.build_owned_reported_with_observer(
            &mut port,
            crate::report_fields::identity(&fields),
            limits,
            scope,
        );
        Rc::new(Attempt {
            inner: RefCell::new(inner),
            fields,
        })
    }

    pub(crate) fn build_selected_with_observer(
        prepared: &mut FileBuild,
        fields: Vec<String>,
        verify: Function,
        resolve_host: Function,
        formatter: Function,
        scope: yamaa_core::application_observer::Scope<'_>,
    ) -> Result<PublicResult, String> {
        let attempt = build_selected_attempt_with_observer(
            prepared,
            fields,
            verify,
            resolve_host,
            yamaa_core::capacity::REPORT_ENCODED_BYTES,
            scope,
        );
        scope.phase(yamaa_core::application_observer::Phase::Output);
        let converted = format_attempt(
            attempt,
            &formatter,
            yamaa_core::capacity::REPORT_ENCODED_BYTES,
            true,
        );
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
                .is_some_and(|result| result.output().is_some())
        }
        pub(crate) fn save(
            &mut self,
        ) -> Result<bool, yamaa_adapters::file_application::FailedBuild> {
            let result = self
                .domain
                .as_mut()
                .ok_or(yamaa_adapters::file_application::FailedBuild)?;
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
                .and_then(|result| result.output_table())
        }
        pub(crate) fn log_table(
            &self,
            warning: bool,
        ) -> Option<&yamaa_adapters::public_table::PublicTable> {
            self.domain
                .as_ref()
                .and_then(|result| result.log_table(warning))
        }
        pub(crate) fn issues(&self) -> Vec<yamaa_adapters::issue_rows::Issue> {
            self.domain.as_ref().map_or_else(
                || {
                    self.refusal_details.as_ref().map_or_else(
                        || yamaa_adapters::file_application::rejected("build", self.refusal),
                        |details| details.issues("build", self.refusal),
                    )
                },
                |result| result.issues().to_vec(),
            )
        }
        pub(crate) fn observations(&self) -> Option<String> {
            self.domain
                .as_ref()
                .map(|result| result.observations_json())
                .or_else(|| self.observations.clone())
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
    fn details(error: &CallbackError) -> Option<HostDetails<'_>> {
        match error {
            CallbackError::Exception {
                class,
                message,
                truncated,
            } => Some(HostDetails::Exception {
                class,
                message,
                truncated: *truncated,
            }),
            CallbackError::Rejected { reason, returned } => Some(HostDetails::Rejected {
                reason,
                returned: returned.as_deref(),
            }),
            CallbackError::Boundary(_) => None,
        }
    }
    fn ordinary(
        condition: &Robj,
        formatter: &Function,
        rejected: bool,
        remaining: &mut usize,
        limited: &mut Option<yamaa_adapters::resource_diagnostics::Refusal>,
        projection_maximum: usize,
    ) -> Option<CallbackError> {
        let reply = formatter
            .call(pairlist!(condition = condition.clone()))
            .ok()?;
        let reply = reply.as_list().filter(|reply| reply.len() == 2)?;
        let class = reply.elt(0).ok()?.as_raw()?;
        let message = reply.elt(1).ok()?.as_raw()?;
        if let Some(length) = [class.len(), message.len()]
            .into_iter()
            .find(|&length| length > yamaa_adapters::callback::MAX_ERROR_BYTES)
        {
            *limited = Some(yamaa_adapters::resource_diagnostics::Refusal::new(
                "host_error_bytes",
                yamaa_adapters::callback::MAX_ERROR_BYTES,
                Some(length),
                Vec::new(),
            ));
            return None;
        }
        let class = std::str::from_utf8(class.as_slice()).ok()?;
        let message = std::str::from_utf8(message.as_slice()).ok()?;
        for text in [class, message] {
            let next = text
                .len()
                .checked_mul(12)
                .and_then(|size| size.checked_add(128))
                .and_then(|size| remaining.checked_sub(size));
            let Some(next) = next else {
                *limited = Some(yamaa_adapters::resource_diagnostics::Refusal::new(
                    "report_bytes",
                    projection_maximum,
                    text.len()
                        .checked_mul(12)
                        .and_then(|size| size.checked_add(128))
                        .and_then(|charge| (projection_maximum - *remaining).checked_add(charge)),
                    Vec::new(),
                ));
                return None;
            };
            *remaining = next;
        }
        Some(if rejected {
            CallbackError::Rejected {
                reason: message.into(),
                returned: None,
            }
        } else {
            CallbackError::Exception {
                class: class.into(),
                message: message.into(),
                truncated: false,
            }
        })
    }
    fn format_attempt(
        attempt: Rc<Attempt>,
        formatter: &Function,
        maximum: usize,
        public: bool,
    ) -> Result<PublicResult, String> {
        // Exclusive borrow spans host classification. Reentrant reads or reports
        // refuse promptly; the original rooted condition cannot be replaced.
        let observations = attempt
            .inner
            .try_borrow_mut()
            .map_err(|_| "producer attempt already borrowed")?;
        let mut count = 0usize;
        yamaa_adapters::producer_attempt::visit_host_failures(&observations, |_, _| {
            count = count.saturating_add(1)
        });
        let projection_maximum = maximum.min(yamaa_adapters::host_limits::REPLY_BYTES);
        let mut remaining = projection_maximum;
        let admitted = count <= yamaa_adapters::host_limits::RECORDS
            && count
                .checked_mul(128)
                .and_then(|size| remaining.checked_sub(size))
                .is_some();
        let mut refusal = if admitted { "" } else { "report_limit" };
        let mut refusal_details = if admitted {
            None
        } else if count > yamaa_adapters::host_limits::RECORDS {
            Some(yamaa_adapters::resource_diagnostics::Refusal::new(
                "report_entries",
                yamaa_adapters::host_limits::RECORDS,
                Some(count),
                Vec::new(),
            ))
        } else {
            Some(yamaa_adapters::resource_diagnostics::Refusal::new(
                "report_bytes",
                projection_maximum,
                count.checked_mul(128),
                Vec::new(),
            ))
        };
        let mut failures = Vec::new();
        let mut projections = Vec::new();
        if admitted {
            remaining -= count * 128;
            failures.reserve(count);
            projections.reserve(count);
            yamaa_adapters::producer_attempt::visit_host_failures(&observations, |_, failure| {
                failures.push(failure)
            });
            for failure in &failures {
                let mut limited = None;
                let projection = match failure {
                    HostError::Host(condition) => ordinary(
                        condition,
                        formatter,
                        false,
                        &mut remaining,
                        &mut limited,
                        projection_maximum,
                    ),
                    HostError::Representation(condition) => ordinary(
                        condition,
                        formatter,
                        true,
                        &mut remaining,
                        &mut limited,
                        projection_maximum,
                    ),
                    HostError::Scalar(reason) => Some(CallbackError::Rejected {
                        reason: (*reason).into(),
                        returned: None,
                    }),
                    _ => None,
                };
                if let Some(details) = limited {
                    refusal_details = Some(details);
                    refusal = "report_limit";
                    break;
                }
                if let Some(details) = projection.as_ref().and_then(details) {
                    let texts = match details {
                        HostDetails::Exception { class, message, .. } => {
                            [Some(class), Some(message)]
                        }
                        HostDetails::Rejected { reason, returned } => [Some(reason), returned],
                    };
                    for text in texts.into_iter().flatten() {
                        let charge = text
                            .len()
                            .checked_mul(12)
                            .and_then(|size| size.checked_add(128));
                        if let Some(left) = charge.and_then(|size| remaining.checked_sub(size)) {
                            remaining = left;
                        } else {
                            refusal = "report_limit";
                            refusal_details =
                                Some(yamaa_adapters::resource_diagnostics::Refusal::new(
                                    "report_bytes",
                                    projection_maximum,
                                    charge.and_then(|charge| {
                                        (projection_maximum - remaining).checked_add(charge)
                                    }),
                                    Vec::new(),
                                ));
                        }
                    }
                }
                projections.push(projection);
                if !refusal.is_empty() {
                    break;
                }
            }
        }
        let mut formatted = None;
        let mut domain = None;
        let mut output_error = None;
        if refusal.is_empty() {
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
            let projected = if public {
                match yamaa_adapters::producer_result::prepare_owned_classified(
                    &observations,
                    crate::report_fields::identity(&attempt.fields),
                    maximum,
                    &facts,
                ) {
                    Ok(value) => {
                        domain = Some(Box::new(value));
                        Ok(None)
                    }
                    Err(yamaa_adapters::producer_result::Error::Report(error)) => Err(error),
                    Err(yamaa_adapters::producer_result::Error::Output {
                        observations: value,
                        error,
                        ..
                    }) => {
                        formatted = Some(value.to_string());
                        if let yamaa_adapters::ipc_table::TableError::Limit {
                            resource,
                            limit,
                            required,
                        } = error
                        {
                            refusal_details =
                                Some(yamaa_adapters::resource_diagnostics::Refusal::new(
                                    resource,
                                    limit,
                                    required,
                                    vec!["output.path".into()],
                                ));
                        }
                        output_error = Some(error);
                        refusal = "output_projection";
                        Ok(None)
                    }
                }
            } else {
                producer_report::attempt_report_classified(
                    &observations,
                    crate::report_fields::identity(&attempt.fields),
                    maximum,
                    &facts,
                )
                .map(Some)
            };
            match projected {
                Ok(Some(value)) => formatted = Some(value.to_string()),
                Ok(None) => {}
                Err(ProjectionError::Original { observations, .. }) => {
                    formatted = Some(observations.to_string());
                    refusal = "original_boundary";
                }
                Err(ProjectionError::Report(
                    yamaa_adapters::specification_report::Error::OutputLimit,
                )) => refusal = "report_limit",
                Err(ProjectionError::Report(
                    yamaa_adapters::specification_report::Error::ResourceLimit(details),
                )) => {
                    refusal = "report_limit";
                    refusal_details = Some(details);
                }
                Err(_) => refusal = "report_refusal",
            }
        }
        drop(observations);
        Ok(PublicResult {
            attempt,
            observations: formatted,
            refusal,
            refusal_details,
            domain,
            output_error,
        })
    }
    fn held_evidence(attempt: &Attempt) -> Result<Robj, String> {
        let held = attempt
            .inner
            .try_borrow()
            .map_err(|_| "producer attempt already borrowed")?;
        let mut count = 0usize;
        let mut host_text = yamaa_adapters::host_limits::REPLY_BYTES;
        let mut records = yamaa_adapters::host_limits::RECORDS;
        let mut admission: Result<(), &'static str> = Ok(());
        yamaa_adapters::producer_attempt::visit_host_failures(&held, |_, failure| {
            if admission.is_err() {
                return;
            }
            count = count.saturating_add(1);
            let mut charge = |text: &str| {
                records = records.checked_sub(1).ok_or("producer evidence limit")?;
                host_text = host_text
                    .checked_sub(text.len())
                    .ok_or("producer evidence limit")?;
                Ok(())
            };
            if let HostError::Lock(findings) = failure {
                for finding in findings {
                    for text in std::iter::once(finding.package.as_str())
                        .chain(finding.expected.iter().map(String::as_str))
                        .chain(finding.actual.as_deref())
                    {
                        if let Err(error) = charge(text) {
                            admission = Err(error);
                            return;
                        }
                    }
                }
            }
        });
        admission?;
        if count > yamaa_adapters::host_limits::RECORDS {
            return Err("producer evidence limit".into());
        }
        let sources = match &held.resources {
            Ok(sources) => sources,
            Err(error) => &error.retained,
        };
        let mut bytes = yamaa_adapters::host_limits::SOURCE_BYTES;
        let mut text = yamaa_adapters::host_limits::REPLY_BYTES;
        if sources.len() > yamaa_adapters::host_limits::RECORDS {
            return Err("producer evidence limit".into());
        }
        for (name, source) in sources {
            bytes = bytes
                .checked_sub(source.len())
                .ok_or("producer evidence limit")?;
            text = text
                .checked_sub(name.len())
                .ok_or("producer evidence limit")?;
        }
        let mut failures = Vec::with_capacity(count);
        // Whole host and source evidence admission precedes mechanical R
        // ownership. This traversal never renders a condition or calls a port.
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
