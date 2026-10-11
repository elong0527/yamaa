//! Registered private graph carriers; original R payloads stay on the calling
//! thread. Pointer identities are registry keys, never cast native pointees.
use crate::specification_service::boundary;
use extendr_api::prelude::*;

#[cfg(any(unix, windows))]
pub(crate) mod platform {
    use super::*;
    use crate::project_activation::Error as HostError;
    use std::{
        cell::{Cell, RefCell},
        collections::BTreeMap,
        rc::{Rc, Weak},
    };
    use yamaa_adapters::{
        file_producer_build::{FileBuild, Limits},
        file_workflow,
        function_transport::CallbackError,
        producer_report::{self, ProjectionError},
        project_function_diagnostics::HostDetails,
        project_report::Classified,
    };
    type NativeAttempt = yamaa_adapters::file_producer_build::OwnedAttempt<HostError>;
    type Prepared = Result<FileBuild, file_workflow::Error>;
    struct Attempt {
        inner: RefCell<NativeAttempt>,
        fields: Vec<String>,
    }
    enum State {
        Specification(RefCell<Prepared>),
        Attempt(Rc<Attempt>),
        Report {
            attempt: Rc<Attempt>,
            observations: Option<String>,
            refusal: &'static str,
            domain: Option<yamaa_adapters::producer_result::RootResult>,
            output_error: Option<yamaa_adapters::table_transport::TableTransportError>,
        },
    }
    thread_local! {
        static HANDLES: RefCell<BTreeMap<usize, Weak<State>>> = const { RefCell::new(BTreeMap::new()) };
        static RESERVED: Cell<usize> = const { Cell::new(0) };
    }
    struct Slot;
    impl Slot {
        fn reserve() -> Result<Self, String> {
            let live = HANDLES.with(|handles| handles.borrow().len());
            RESERVED.with(|reserved| {
                if live.saturating_add(reserved.get()) >= 4096 {
                    return Err("producer handle limit".into());
                }
                reserved.set(reserved.get() + 1);
                Ok(Self)
            })
        }
    }
    impl Drop for Slot {
        fn drop(&mut self) {
            RESERVED.with(|reserved| reserved.set(reserved.get() - 1));
        }
    }
    struct Handle {
        state: Rc<State>,
        identity: Cell<usize>,
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            HANDLES.with(|handles| {
                handles.borrow_mut().remove(&self.identity.get());
            });
        }
    }
    fn register(state: State, _slot: Slot) -> Result<Robj, String> {
        let handle = ExternalPtr::new(Handle {
            state: Rc::new(state),
            identity: Cell::new(0),
        });
        let identity = crate::specification_service::address(handle.as_robj())?;
        handle.identity.set(identity);
        HANDLES.with(|handles| {
            handles
                .borrow_mut()
                .insert(identity, Rc::downgrade(&handle.state));
        });
        Ok(handle.into_robj())
    }
    fn resolve(handle: &Robj) -> Result<Rc<State>, String> {
        let identity = crate::specification_service::address(handle)?;
        HANDLES
            .with(|handles| handles.borrow().get(&identity).and_then(Weak::upgrade))
            .ok_or_else(|| "unknown producer handle".into())
    }
    fn attempt(state: &State) -> Result<&Rc<Attempt>, String> {
        match state {
            State::Attempt(attempt) | State::Report { attempt, .. } => Ok(attempt),
            _ => Err("producer attempt required".into()),
        }
    }
    fn path(raw: &Raw) -> Result<&str, String> {
        if raw.len() > 65536 {
            return Err("producer path byte limit".into());
        }
        std::str::from_utf8(raw.as_slice()).map_err(|_| "invalid producer path UTF-8".into())
    }
    pub(super) fn prepare(specification: Raw, environment: Raw) -> Result<Robj, String> {
        let slot = Slot::reserve()?;
        let prepared = file_workflow::prepare(
            path(&specification)?,
            path(&environment)?,
            yamaa_core::project_function::Language::R,
        )
        .map(|graph| graph.into_build());
        register(State::Specification(RefCell::new(prepared)), slot)
    }
    pub(super) fn status(handle: Robj) -> Result<Robj, String> {
        let state = resolve(&handle)?;
        Ok(r!(match state.as_ref() {
            State::Specification(state) => match &*state
                .try_borrow()
                .map_err(|_| "producer handle already borrowed")?
            {
                Ok(_) => "ready",
                Err(file_workflow::Error::Configuration(_)) => "configuration_failure",
                Err(file_workflow::Error::Schema(_)) => "schema_failure",
                Err(file_workflow::Error::Admission(_)) => "admission_failure",
                Err(file_workflow::Error::Environment(_)) => "environment_failure",
                Err(file_workflow::Error::Graph(_)) => "graph_failure",
            },
            State::Attempt(_) => "attempt",
            State::Report { refusal: "", .. } => "complete",
            State::Report { refusal, .. } => refusal,
        }))
    }
    pub(super) fn build(
        handle: Robj,
        metadata: List,
        verify: Function,
        resolve_host: Function,
        report_bytes: i32,
    ) -> Result<Robj, String> {
        let slot = Slot::reserve()?;
        let fields = crate::specification_result::fields(&metadata)?;
        let maximum = usize::try_from(report_bytes).map_err(|_| "invalid report byte limit")?;
        let owner = resolve(&handle)?;
        let State::Specification(state) = owner.as_ref() else {
            return Err("producer specification required".into());
        };
        let mut state = state
            .try_borrow_mut()
            .map_err(|_| "producer handle already borrowed")?;
        let prepared = state
            .as_mut()
            .map_err(|_| "producer preparation failed; original rejection remains retained")?;
        register(
            State::Attempt(build_selected_attempt(
                prepared,
                fields,
                verify,
                resolve_host,
                maximum,
            )),
            slot,
        )
    }
    fn build_selected_attempt(
        prepared: &mut FileBuild,
        fields: Vec<String>,
        verify: Function,
        resolve_host: Function,
        maximum: usize,
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
        let inner = prepared.build_owned_reported(
            &mut port,
            crate::specification_result::identity(&fields),
            limits,
        );
        Rc::new(Attempt {
            inner: RefCell::new(inner),
            fields,
        })
    }

    /// The public domain registry roots this whole state on the calling thread.
    /// Its reservation continues for the full result lifetime, before growth.
    pub(crate) struct PublicResult {
        state: State,
        _slot: Slot,
    }
    pub(crate) fn build_selected(
        prepared: &mut FileBuild,
        fields: Vec<String>,
        verify: Function,
        resolve_host: Function,
        formatter: Function,
    ) -> Result<PublicResult, String> {
        let slot = Slot::reserve()?;
        let attempt = build_selected_attempt(prepared, fields, verify, resolve_host, 16_777_216);
        Ok(PublicResult {
            state: format_attempt(attempt, &formatter, 16_777_216, true)?,
            _slot: slot,
        })
    }
    impl PublicResult {
        pub(crate) fn save(
            &mut self,
        ) -> Result<bool, yamaa_adapters::file_application::FailedBuild> {
            let State::Report {
                domain: Some(result),
                attempt,
                ..
            } = &mut self.state
            else {
                return Err(yamaa_adapters::file_application::FailedBuild);
            };
            let _owner = attempt
                .inner
                .try_borrow_mut()
                .map_err(|_| yamaa_adapters::file_application::FailedBuild)?;
            result.save()
        }
        pub(crate) fn output_table(&self) -> Option<&yamaa_adapters::public_table::PublicTable> {
            match &self.state {
                State::Report {
                    domain: Some(result),
                    ..
                } => result.output_table(),
                _ => None,
            }
        }
        pub(crate) fn issues(&self) -> Vec<yamaa_adapters::issue_rows::Issue> {
            match &self.state {
                State::Report {
                    domain: Some(result),
                    ..
                } => result.issues().to_vec(),
                State::Report { refusal, .. } => {
                    yamaa_adapters::file_application::rejected("build", refusal)
                }
                _ => yamaa_adapters::file_application::rejected("build", "producer_report"),
            }
        }
        pub(crate) fn observations(&self) -> Option<String> {
            match &self.state {
                State::Report {
                    domain: Some(result),
                    ..
                } => Some(result.observations().to_string()),
                State::Report { observations, .. } => observations.clone(),
                _ => None,
            }
        }
        pub(crate) fn projection_failure(&self) -> Option<String> {
            match &self.state {
                State::Report { output_error, .. } => output_error.map(|error| error.to_string()),
                _ => None,
            }
        }
        pub(crate) fn interrupt(&self) -> Result<Robj, String> {
            held_interrupt(attempt(&self.state)?)
        }
        pub(crate) fn evidence(&self) -> Result<Robj, String> {
            held_evidence(attempt(&self.state)?)
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
        limited: &mut bool,
    ) -> Option<CallbackError> {
        let reply = formatter
            .call(pairlist!(condition = condition.clone()))
            .ok()?;
        let reply = reply.as_list().filter(|reply| reply.len() == 2)?;
        let class = reply.elt(0).ok()?.as_raw()?;
        let message = reply.elt(1).ok()?.as_raw()?;
        if [class.len(), message.len()]
            .iter()
            .any(|&length| length > yamaa_adapters::function_transport::MAX_ERROR_BYTES)
        {
            *limited = true;
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
                *limited = true;
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
    pub(super) fn report(handle: Robj, formatter: Function, maximum: i32) -> Result<Robj, String> {
        let slot = Slot::reserve()?;
        let maximum = usize::try_from(maximum).map_err(|_| "invalid report byte limit")?;
        let owner = resolve(&handle)?;
        let attempt = Rc::clone(attempt(&owner)?);
        register(format_attempt(attempt, &formatter, maximum, false)?, slot)
    }
    fn format_attempt(
        attempt: Rc<Attempt>,
        formatter: &Function,
        maximum: usize,
        public: bool,
    ) -> Result<State, String> {
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
        let mut remaining = maximum.min(16_777_216);
        let admitted = count <= 65536
            && count
                .checked_mul(128)
                .and_then(|size| remaining.checked_sub(size))
                .is_some();
        let mut refusal = if admitted { "" } else { "report_limit" };
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
                let mut limited = false;
                let projection = match failure {
                    HostError::Host(condition) => {
                        ordinary(condition, formatter, false, &mut remaining, &mut limited)
                    }
                    HostError::Representation(condition) => {
                        ordinary(condition, formatter, true, &mut remaining, &mut limited)
                    }
                    HostError::Scalar(reason) => Some(CallbackError::Rejected {
                        reason: (*reason).into(),
                        returned: None,
                    }),
                    _ => None,
                };
                if limited {
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
                        if let Some(left) = text
                            .len()
                            .checked_mul(12)
                            .and_then(|size| size.checked_add(128))
                            .and_then(|size| remaining.checked_sub(size))
                        {
                            remaining = left;
                        } else {
                            refusal = "report_limit";
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
                    crate::specification_result::identity(&attempt.fields),
                    maximum,
                    &facts,
                ) {
                    Ok(value) => {
                        domain = Some(value);
                        Ok(None)
                    }
                    Err(yamaa_adapters::producer_result::Error::Report(error)) => Err(error),
                    Err(yamaa_adapters::producer_result::Error::Output {
                        observations: value,
                        error,
                        ..
                    }) => {
                        formatted = Some(value.to_string());
                        output_error = Some(error);
                        refusal = "output_projection";
                        Ok(None)
                    }
                }
            } else {
                producer_report::attempt_report_classified(
                    &observations,
                    crate::specification_result::identity(&attempt.fields),
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
                Err(_) => refusal = "report_refusal",
            }
        }
        drop(observations);
        Ok(State::Report {
            attempt,
            observations: formatted,
            refusal,
            domain,
            output_error,
        })
    }
    pub(super) fn observations(handle: Robj) -> Result<Robj, String> {
        let state = resolve(&handle)?;
        match state.as_ref() {
            State::Report {
                observations: Some(value),
                ..
            } => Ok(r!(value.as_str())),
            _ => Err("report refused; original attempt remains retained".into()),
        }
    }
    pub(super) fn retained(handle: Robj) -> Result<Robj, String> {
        let owner = resolve(&handle)?;
        held_evidence(attempt(&owner)?)
    }
    fn held_evidence(attempt: &Attempt) -> Result<Robj, String> {
        let held = attempt
            .inner
            .try_borrow()
            .map_err(|_| "producer attempt already borrowed")?;
        let mut count = 0usize;
        let mut host_text = 16_777_216usize;
        let mut records = 65_536usize;
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
        if count > 65536 {
            return Err("producer evidence limit".into());
        }
        let sources = match &held.resources {
            Ok(sources) => sources,
            Err(error) => &error.retained,
        };
        let mut bytes = 67_108_864usize;
        let mut text = 16_777_216usize;
        if sources.len() > 65536 {
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
    pub(super) fn interrupt(handle: Robj) -> Result<Robj, String> {
        let owner = resolve(&handle)?;
        held_interrupt(attempt(&owner)?)
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
    pub(super) fn rejection_issues(handle: Robj, maximum: i32) -> Result<Robj, String> {
        let maximum = usize::try_from(maximum).map_err(|_| "invalid report byte limit")?;
        let owner = resolve(&handle)?;
        let State::Specification(state) = owner.as_ref() else {
            return Err("producer specification required".into());
        };
        let state = state
            .try_borrow()
            .map_err(|_| "producer handle already borrowed")?;
        let error = state
            .as_ref()
            .err()
            .ok_or("producer preparation has no rejection")?;
        let issues = yamaa_adapters::producer_check::rejected_with_limit(
            error,
            yamaa_core::project_function::Language::R,
            maximum,
        )
        .map_err(|_| "rejection projection refused; original rejection remains retained")?;
        crate::issue_frame::frame(&issues)
    }
    pub(super) fn rejected_sources(handle: Robj) -> Result<Robj, String> {
        let owner = resolve(&handle)?;
        let State::Specification(state) = owner.as_ref() else {
            return Err("producer specification required".into());
        };
        let state = state
            .try_borrow()
            .map_err(|_| "producer handle already borrowed")?;
        let sources = match &*state {
            Err(file_workflow::Error::Admission(error)) => error.retained_sources(65536, 16777216),
            Err(file_workflow::Error::Environment(error)) => {
                error.retained_sources(65536, 16777216)
            }
            Err(file_workflow::Error::Graph(error)) => error.retained_sources(65536, 16777216),
            _ => Ok(Vec::new()),
        }
        .map_err(|_| "source evidence limit; original rejection remains retained")?;
        let mut bytes = 67_108_864usize;
        for (_, source) in &sources {
            bytes = bytes
                .checked_sub(source.len())
                .ok_or("source evidence limit")?;
        }
        Ok(List::from_values(sources.iter().map(|(name, bytes)| {
            list!(
                name = Raw::from_bytes(name.as_bytes()),
                bytes = Raw::from_bytes(bytes)
            )
        }))
        .into_robj())
    }
    pub(super) fn artifact(handle: Robj, node: i32) -> Result<Robj, String> {
        let node = usize::try_from(node).map_err(|_| "invalid node index")?;
        let owner = resolve(&handle)?;
        let held = attempt(&owner)?
            .inner
            .try_borrow()
            .map_err(|_| "producer attempt already borrowed")?;
        Ok(held
            .graph
            .nodes
            .iter()
            .find(|entered| entered.node == node)
            .and_then(|entered| entered.output.as_ref().ok())
            .and_then(Option::as_ref)
            .and_then(|output| output.artifact())
            .map_or_else(
                || r!(NULL),
                |artifact| Raw::from_bytes(artifact.bytes()).into_robj(),
            ))
    }
}

#[extendr]
fn prepare_producer_file(specification: Raw, environment: Raw) -> List {
    boundary(|| {
        #[cfg(any(unix, windows))]
        {
            platform::prepare(specification, environment)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (specification, environment);
            Err("native producer preparation unavailable on this platform".into())
        }
    })
}
#[extendr]
fn producer_status(handle: Robj) -> List {
    boundary(|| {
        #[cfg(any(unix, windows))]
        {
            platform::status(handle)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = handle;
            Err("native producer handles unavailable".into())
        }
    })
}
#[extendr]
fn producer_build(
    handle: Robj,
    metadata: List,
    verify: Function,
    resolve: Function,
    report_bytes: i32,
) -> List {
    boundary(|| {
        #[cfg(any(unix, windows))]
        {
            platform::build(handle, metadata, verify, resolve, report_bytes)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (handle, metadata, verify, resolve, report_bytes);
            Err("native producer builds unavailable".into())
        }
    })
}
#[extendr]
fn producer_report(handle: Robj, formatter: Function, maximum: i32) -> List {
    boundary(|| {
        #[cfg(any(unix, windows))]
        {
            platform::report(handle, formatter, maximum)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (handle, formatter, maximum);
            Err("native producer reports unavailable".into())
        }
    })
}
#[extendr]
fn producer_observations(handle: Robj) -> List {
    boundary(|| {
        #[cfg(any(unix, windows))]
        {
            platform::observations(handle)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = handle;
            Err("native producer reports unavailable".into())
        }
    })
}
#[extendr]
fn producer_retained(handle: Robj) -> List {
    boundary(|| {
        #[cfg(any(unix, windows))]
        {
            platform::retained(handle)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = handle;
            Err("native producer evidence unavailable".into())
        }
    })
}
#[extendr]
fn producer_interrupt(handle: Robj) -> List {
    boundary(|| {
        #[cfg(any(unix, windows))]
        {
            platform::interrupt(handle)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = handle;
            Err("native producer evidence unavailable".into())
        }
    })
}
#[extendr]
fn producer_artifact(handle: Robj, node: i32) -> List {
    boundary(|| {
        #[cfg(any(unix, windows))]
        {
            platform::artifact(handle, node)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (handle, node);
            Err("native producer evidence unavailable".into())
        }
    })
}
#[extendr]
fn producer_rejected_sources(handle: Robj) -> List {
    boundary(|| {
        #[cfg(any(unix, windows))]
        {
            platform::rejected_sources(handle)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = handle;
            Err("native producer evidence unavailable".into())
        }
    })
}
#[extendr]
fn producer_rejection_issues(handle: Robj, maximum: i32) -> List {
    boundary(|| {
        #[cfg(any(unix, windows))]
        {
            platform::rejection_issues(handle, maximum)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (handle, maximum);
            Err("native producer evidence unavailable".into())
        }
    })
}
extendr_module! { mod file_producer; fn prepare_producer_file; fn producer_status; fn producer_build; fn producer_report; fn producer_observations; fn producer_retained; fn producer_interrupt; fn producer_artifact; fn producer_rejected_sources; fn producer_rejection_issues; }
