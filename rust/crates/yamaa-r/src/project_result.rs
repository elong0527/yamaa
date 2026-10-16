//! Classify retained ordinary-project failures and format the public result.
use extendr_api::prelude::*;
pub(crate) mod platform {
    use super::*;
    use crate::{file_project::platform::HeldAttempt, project_activation::Error as HostError};
    use std::rc::Rc;
    use yamaa_adapters::{
        callback::{details, CallbackError},
        file_application::BuildRefusal,
        project_report::{self, Classified},
        specification_report::BuildResult,
    };
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
    ) -> Result<BuildResult, BuildRefusal> {
        let mut failures = Vec::new();
        yamaa_adapters::project_attempt::visit_host_failures(&attempt.inner, |_, error| {
            failures.push(error)
        });
        let mut projections = Vec::new();
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
        project_report::build_result(
            &attempt.run,
            &attempt.inner,
            crate::report_fields::identity(fields),
            &facts,
        )
        .map_err(|error| error.refusal())
    }
}
