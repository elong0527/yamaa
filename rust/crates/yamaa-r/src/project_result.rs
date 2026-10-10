//! Registered complete project results retain every original condition and source.
use crate::specification_service::boundary;
use extendr_api::prelude::*;

#[cfg(unix)]
mod platform {
    use super::*;
    use crate::{file_project::platform::HeldAttempt, project_activation::Error as HostError};
    use std::{
        cell::{Cell, RefCell},
        collections::BTreeMap,
        rc::{Rc, Weak},
    };
    use yamaa_adapters::{
        function_transport::CallbackError,
        project_function_diagnostics::HostDetails,
        project_report::{self, Classified},
        specification_report::BuildResult,
    };
    struct HeldResult {
        attempt: Rc<HeldAttempt>,
        formatted: Option<BuildResult>,
        refusal: &'static str,
    }
    thread_local! {
        static RESULTS: RefCell<BTreeMap<usize, Weak<HeldResult>>> = const { RefCell::new(BTreeMap::new()) };
    }
    struct Handle {
        state: Rc<HeldResult>,
        identity: Cell<usize>,
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            RESULTS.with(|handles| {
                handles.borrow_mut().remove(&self.identity.get());
            });
        }
    }
    fn resolve(handle: &Robj) -> std::result::Result<Rc<HeldResult>, String> {
        let id = crate::specification_service::address(handle)?;
        RESULTS
            .with(|handles| handles.borrow().get(&id).and_then(Weak::upgrade))
            .ok_or_else(|| "unknown project result handle".into())
    }
    fn report(state: &HeldResult) -> std::result::Result<&BuildResult, String> {
        state.formatted.as_ref().ok_or_else(|| {
            "complete project report is unavailable; original attempt remains retained".into()
        })
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
    pub(super) fn prepare(
        handle: Robj,
        metadata: List,
        formatter: Function,
    ) -> std::result::Result<Robj, String> {
        let fields = crate::specification_result::fields(&metadata)?;
        let attempt = crate::file_project::platform::attempt(&handle)?;
        let mut failures = Vec::new();
        yamaa_adapters::project_attempt::visit_host_failures(&attempt.inner, |_, error| {
            failures.push(error)
        });
        let mut projections = Vec::new();
        let mut budget = yamaa_adapters::specification_check::MAX_ISSUE_BYTES;
        let mut refusal = "";
        for failure in &failures {
            let projection = match failure {
                HostError::Host(condition) => ordinary(condition, &formatter, false),
                HostError::Representation(condition) => ordinary(condition, &formatter, true),
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
                crate::specification_result::identity(&fields),
                &facts,
            ) {
                Ok(result) => Some(result),
                Err(error) => {
                    refusal = match error {
                        project_report::Error::Opaque(_) => "opaque_host",
                        project_report::Error::Boundary(_) => "activation_boundary",
                        project_report::Error::Source(_) => "source_boundary",
                        project_report::Error::Execution(_) => "execution_boundary",
                        project_report::Error::Report(
                            yamaa_adapters::specification_report::Error::OutputLimit,
                        ) => "report_limit",
                        project_report::Error::Report(_) => "report_refusal",
                    };
                    None
                }
            }
        } else {
            None
        };
        let handle = ExternalPtr::new(Handle {
            state: Rc::new(HeldResult {
                attempt,
                formatted,
                refusal,
            }),
            identity: Cell::new(0),
        });
        let id = crate::specification_service::address(handle.as_robj())?;
        handle.identity.set(id);
        RESULTS.with(|handles| {
            handles
                .borrow_mut()
                .insert(id, Rc::downgrade(&handle.state));
        });
        Ok(handle.into_robj())
    }
    pub(super) fn status(handle: Robj) -> std::result::Result<Robj, String> {
        let state = resolve(&handle)?;
        Ok(r!(if state.formatted.is_some() {
            "complete"
        } else {
            state.refusal
        }))
    }
    pub(super) fn output(handle: Robj) -> std::result::Result<Robj, String> {
        let state = resolve(&handle)?;
        Ok(report(&state)?
            .output()
            .map_or_else(|| r!(NULL), |bytes| Raw::from_bytes(bytes).into_robj()))
    }
    pub(super) fn observations(handle: Robj) -> std::result::Result<Robj, String> {
        let state = resolve(&handle)?;
        Ok(r!(report(&state)?.observations().to_string()))
    }
    pub(super) fn issues(handle: Robj) -> std::result::Result<Robj, String> {
        let state = resolve(&handle)?;
        crate::issue_frame::frame(report(&state)?.issues())
    }
    pub(super) fn retained(handle: Robj) -> std::result::Result<Robj, String> {
        crate::file_project::platform::observe(&resolve(&handle)?.attempt)
    }
    pub(super) fn interrupt(handle: Robj) -> std::result::Result<Robj, String> {
        let state = resolve(&handle)?;
        let mut interrupted = None;
        yamaa_adapters::project_attempt::visit_host_failures(&state.attempt.inner, |_, error| {
            if let HostError::Interrupt(condition) = error {
                if interrupted.is_none() {
                    interrupted = Some(condition.clone());
                }
            }
        });
        Ok(interrupted.unwrap_or_else(|| r!(NULL)))
    }
    pub(super) fn save(handle: Robj, publish: Function) -> std::result::Result<Robj, String> {
        let state = resolve(&handle)?;
        report(&state)?
            .save(&mut crate::specification_result::Publisher(publish))
            .map(|report| r!(report.to_string()))
            .map_err(|error| match error {
                yamaa_engine::specification_output::SaveError::FailedBuild => {
                    "cannot save a failed build".into()
                }
                yamaa_engine::specification_output::SaveError::Publish(error) => error,
            })
    }
    pub(super) fn save_file(handle: Robj, publisher: Robj) -> std::result::Result<Robj, String> {
        let state = resolve(&handle)?;
        crate::file_publication::platform::save_result(report(&state)?, publisher)
    }
}

// Non-Unix platforms retain the existing explicit R support boundary.
macro_rules! operation {
    ($name:ident, $operation:ident, $( $argument:ident : $kind:ty ),+) => {
        #[extendr]
        fn $name($( $argument: $kind ),+) -> List {
            boundary(|| {
                #[cfg(unix)] { platform::$operation($( $argument ),+) }
                #[cfg(not(unix))] {
                    let _ = ($( $argument ),+);
                    Err("native project results unavailable on this platform".into())
                }
            })
        }
    };
}
operation!(file_project_result, prepare, handle: Robj, metadata: List, formatter: Function);
operation!(project_result_status, status, handle: Robj);
operation!(project_result_output, output, handle: Robj);
operation!(project_result_observations, observations, handle: Robj);
operation!(project_result_issues, issues, handle: Robj);
operation!(project_result_retained, retained, handle: Robj);
operation!(project_result_interrupt, interrupt, handle: Robj);
operation!(project_result_save, save, handle: Robj, publish: Function);
operation!(project_result_save_file, save_file, handle: Robj, publisher: Robj);
extendr_module! {
    mod project_result;
    fn file_project_result;
    fn project_result_status;
    fn project_result_output;
    fn project_result_observations;
    fn project_result_issues;
    fn project_result_retained;
    fn project_result_interrupt;
    fn project_result_save;
    fn project_result_save_file;
}
