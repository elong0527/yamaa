//! Classify retained ordinary-project failures and format the public result.
use extendr_api::prelude::*;
pub(crate) mod platform {
    use super::*;
    use crate::{file_project::platform::HeldAttempt, project_activation::Error as HostError};
    use std::rc::Rc;
    use yamaa_adapters::{
        callback::{details, CallbackError},
        project_report::{self, Classified},
        specification_report::BuildResult,
    };
    pub(crate) struct HeldResult {
        pub(crate) formatted: Option<BuildResult>,
        pub(crate) refusal: &'static str,
        pub(crate) refusal_details: Option<yamaa_adapters::resource_diagnostics::Refusal>,
    }
    // The installed helper renders only explicitly ordinary conditions. A failed
    // secondary renderer cannot replace the original payload in HeldAttempt.
    pub(crate) fn ordinary(
        condition: &Robj,
        formatter: &Function,
        rejected: bool,
    ) -> Option<CallbackError> {
        let reply = formatter
            .call(pairlist!(condition = condition.clone()))
            .ok()?;
        let reply = reply.as_list().filter(|reply| reply.len() == 2)?;
        let class = crate::function_callback::detail(&reply.elt(0).ok()?, "R condition").ok()?;
        let message = crate::function_callback::detail(
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
            CallbackError::Exception { class, message }
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
            projections.push(projection);
        }
        let formatted = {
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
                            yamaa_adapters::specification_report::Error::OutputLimit
                            | yamaa_adapters::specification_report::Error::ResourceLimit(_),
                        ) => "output_limit",
                        project_report::Error::Report(_) => "report_refusal",
                    };
                    None
                }
            }
        };
        HeldResult {
            formatted,
            refusal,
            refusal_details,
        }
    }
}
