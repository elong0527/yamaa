//! Classify retained ordinary-project failures and format the public result.
use extendr_api::prelude::*;
pub(crate) mod platform {
    use super::*;
    use crate::{file_project::platform::HeldAttempt, project_activation::Error as HostError};
    use std::rc::Rc;
    use yamaa_adapters::{
        callback::CallbackError,
        project_function_diagnostics::HostDetails,
        project_report::{self, Classified},
        specification_report::BuildResult,
    };
    pub(crate) struct HeldResult {
        pub(crate) formatted: Option<BuildResult>,
        pub(crate) refusal: &'static str,
        pub(crate) refusal_details: Option<yamaa_adapters::resource_diagnostics::Refusal>,
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
    // The installed helper renders only explicitly ordinary conditions. A failed
    // secondary renderer cannot replace the original payload in HeldAttempt.
    fn ordinary(condition: &Robj, formatter: &Function, rejected: bool) -> Option<CallbackError> {
        let reply = formatter
            .call(pairlist!(condition = condition.clone()))
            .ok()?;
        let reply = reply.as_list().filter(|reply| reply.len() == 2)?;
        let (class, class_cut) =
            crate::function_callback::detail(&reply.elt(0).ok()?, "R condition").ok()?;
        let (message, message_cut) = crate::function_callback::detail(
            &reply.elt(1).ok()?,
            "unavailable R condition message",
        )
        .ok()?;
        Some(if rejected {
            CallbackError::Rejected {
                reason: message,
                returned: None,
            }
        } else {
            CallbackError::Exception {
                class,
                message,
                truncated: class_cut || message_cut,
            }
        })
    }
    pub(crate) fn format(
        attempt: Rc<HeldAttempt>,
        fields: &[String],
        formatter: &Function,
    ) -> HeldResult {
        let mut failures = Vec::new();
        yamaa_adapters::project_attempt::visit_host_failures(&attempt.inner, |_, error| {
            failures.push(error)
        });
        let mut projections = Vec::new();
        let mut budget = yamaa_adapters::specification_check::MAX_ISSUE_BYTES;
        let mut refusal = "";
        let mut refusal_details = None;
        for failure in &failures {
            let projection = match failure {
                HostError::Host(condition) => ordinary(condition, formatter, false),
                HostError::Representation(condition) => ordinary(condition, formatter, true),
                HostError::Scalar(reason) => Some(CallbackError::Rejected {
                    reason: (*reason).into(),
                    returned: None,
                }),
                _ => None,
            };
            if let Some(facts) = projection.as_ref().and_then(details) {
                let texts = match facts {
                    HostDetails::Exception { class, message, .. } => [Some(class), Some(message)],
                    HostDetails::Rejected { reason, returned } => [Some(reason), returned],
                };
                for text in texts.into_iter().flatten() {
                    let Some(remaining) = text
                        .len()
                        .checked_mul(12)
                        .and_then(|size| size.checked_add(128))
                        .and_then(|charge| budget.checked_sub(charge))
                    else {
                        refusal = "report_limit";
                        refusal_details = Some(yamaa_adapters::resource_diagnostics::Refusal::new(
                            "diagnostic_bytes",
                            yamaa_adapters::specification_check::MAX_ISSUE_BYTES,
                            text.len()
                                .checked_mul(12)
                                .and_then(|n| n.checked_add(128))
                                .and_then(|charge| {
                                    (yamaa_adapters::specification_check::MAX_ISSUE_BYTES - budget)
                                        .checked_add(charge)
                                }),
                            Vec::new(),
                        ));
                        break;
                    };
                    budget = remaining;
                }
            }
            projections.push(projection);
            if !refusal.is_empty() {
                break;
            }
        }
        let formatted = if refusal.is_empty() {
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
            match project_report::build_result(
                &attempt.run,
                &attempt.inner,
                crate::report_fields::identity(fields),
                &facts,
            ) {
                Ok(result) => Some(result),
                Err(error) => {
                    if let project_report::Error::Report(
                        yamaa_adapters::specification_report::Error::ResourceLimit(details),
                    ) = &error
                    {
                        refusal_details = Some(details.clone());
                    }
                    refusal = match error {
                        project_report::Error::Opaque(_) => "opaque_host",
                        project_report::Error::Boundary(_) => "activation_boundary",
                        project_report::Error::Source(_) => "source_boundary",
                        project_report::Error::Execution(_) => "execution_boundary",
                        project_report::Error::Report(
                            yamaa_adapters::specification_report::Error::OutputLimit,
                        ) => "report_limit",
                        project_report::Error::Report(
                            yamaa_adapters::specification_report::Error::ResourceLimit(_),
                        ) => "report_limit",
                        project_report::Error::Report(_) => "report_refusal",
                    };
                    None
                }
            }
        } else {
            None
        };
        HeldResult {
            formatted,
            refusal,
            refusal_details,
        }
    }
}
