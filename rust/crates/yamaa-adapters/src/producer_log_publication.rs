//! Diagnostic and primary publication sealed beside one actual native build.
//! Failed graphs retain verification-only authority; public attempts grant none.
use crate::{
    file_producer_build::Provenance,
    file_resources::{Error as ResourceError, Resources},
    producer_publication::{same_location, AdmissionFailure},
    producer_report::{NativeAttempt, NodeLogs},
    report_storage::StoredReport,
    resource_publication::{AnchoredTarget, Publisher},
    specification_report::Error as ReportError,
};
use std::{
    any::Any,
    collections::VecDeque,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
};
use yamaa_core::json::Value;
use yamaa_core::producer_publication::PreparedPublication;

/// Lexical collisions across the complete authored graph are metadata facts.
/// Existing-parent/physical alias checks remain at Save's whole-set preflight.
pub(crate) fn admit_locations(
    documents: &[Arc<crate::specification_source::PreparedDocument>],
    resources: &Resources,
) -> Result<(), crate::file_graph::Error> {
    use crate::file_graph::Error;
    use yamaa_core::specification::{LogFinding, PreflightFinding, PrepareError};
    let fields = [
        ("path", "output.path"),
        ("verification_log", "output.verification_log"),
        ("warning_log", "output.warning_log"),
    ];
    if !documents.iter().any(|document| {
        fields[1..]
            .iter()
            .any(|(field, _)| document.written_output_field_origin(field).is_some())
    }) {
        return Ok(());
    }
    let count = documents
        .iter()
        .map(|document| {
            fields
                .iter()
                .filter(|(field, _)| document.written_output_field_origin(field).is_some())
                .count()
        })
        .sum::<usize>();
    let mut locations: Vec<(&'static str, String)> = Vec::with_capacity(count);
    for (node, document) in documents.iter().enumerate() {
        for (field, right) in fields {
            if let Some(origin) = document.written_output_field_origin(field) {
                // Match ordinary metadata admission: an operational path
                // refusal does not require output parents to exist at Build.
                let Ok(location) = resources.location_from(origin.base_source, origin.written)
                else {
                    continue;
                };
                if let Some((left, _)) = locations
                    .iter()
                    .find(|(_, prior)| same_location(prior, &location))
                {
                    let finding = PreflightFinding::Log(LogFinding::Collision {
                        left,
                        right,
                        path: origin.written.into(),
                    });
                    return Err(Error::Graph(crate::producer_graph::Error::Graph(
                        yamaa_core::producer_graph::Error::Compilation {
                            node,
                            error: PrepareError::Invalid(vec![finding]),
                        },
                    )));
                }
                locations.push((right, location));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Source {
    Primary(usize),
    Verification(usize),
    Warning(usize),
}
pub(crate) struct Request {
    source: Source,
    pub(crate) node: usize,
    pub(crate) field: &'static str,
    pub(crate) path: String,
    pub(crate) observation: Value,
}
struct Borrowed<'a> {
    source: Source,
    node: usize,
    field: &'static str,
    path: &'a str,
    observation: &'a Value,
    specification: &'a str,
}
struct Target {
    request: usize,
    selected: String,
    target: AnchoredTarget,
}
struct Targets {
    values: Vec<Target>,
}
pub(crate) struct Admission {
    pub(crate) failure: AdmissionFailure,
    pub(crate) request: Option<usize>,
}
impl Admission {
    fn unlocated(failure: AdmissionFailure) -> Self {
        Self {
            failure,
            request: None,
        }
    }
    fn at(request: usize, failure: AdmissionFailure) -> Self {
        Self {
            failure,
            request: Some(request),
        }
    }
}
pub(crate) struct Publication {
    primary: Option<PreparedPublication>,
    logs: Arc<[NodeLogs]>,
    requests: Vec<Request>,
    full: Result<Targets, Admission>,
    // A rejected warning/primary target cannot consume a retained V capability.
    // This second set exists only after full admission failed and dropped its
    // target prefix. A successful full set is borrowed for diagnostic Save.
    diagnostic: Option<Result<Targets, Admission>>,
}
#[derive(Debug)]
pub(crate) struct TargetFailure {
    pub(crate) request: usize,
    pub(crate) error: crate::file_publication::Error,
}
#[derive(Debug)]
pub(crate) enum Failure {
    Incomplete,
    Admission,
    Preflight(TargetFailure),
    Publish(TargetFailure),
}
pub(crate) struct Entered {
    pub(crate) request: usize,
    pub(crate) node: usize,
    pub(crate) field: &'static str,
    pub(crate) path: String,
    pub(crate) published: bool,
}
pub(crate) struct Attempt {
    pub(crate) entered: Vec<Entered>,
    pub(crate) outcome: Result<(), Failure>,
    pub(crate) unwind: Option<Box<dyn Any + Send>>,
}
impl Default for Attempt {
    fn default() -> Self {
        Self {
            entered: Vec::new(),
            outcome: Err(Failure::Incomplete),
            unwind: None,
        }
    }
}

fn each<E>(
    attempt: &NativeAttempt<E>,
    primary: Option<&PreparedPublication>,
    logs: &[NodeLogs],
    mut visit: impl FnMut(Borrowed<'_>) -> Result<(), AdmissionFailure>,
) -> Result<(), AdmissionFailure> {
    for &node in attempt.provenance.metadata().order() {
        let specification = attempt.provenance.metadata().nodes()[node].identity();
        if let Some((index, held)) = logs.iter().enumerate().find(|(_, held)| held.node == node) {
            if let Some(log) = &held.logs.verification {
                visit(Borrowed {
                    source: Source::Verification(index),
                    node,
                    field: "verification_log",
                    path: log.path(),

                    observation: log.observation(),
                    specification,
                })?;
            }
            if primary.is_some() {
                if let Some(log) = &held.logs.warning {
                    visit(Borrowed {
                        source: Source::Warning(index),
                        node,
                        field: "warning_log",
                        path: log.path(),

                        observation: log.observation(),
                        specification,
                    })?;
                }
            }
        }
        if let Some(primary) = primary {
            let (index, artifact) = primary
                .artifacts()
                .iter()
                .enumerate()
                .find(|(_, artifact)| artifact.node() == node)
                .ok_or(AdmissionFailure::Incomplete)?;
            let observation = attempt
                .graph
                .nodes
                .iter()
                .find(|entered| entered.node == node)
                .and_then(|entered| entered.output.as_ref().ok())
                .and_then(Option::as_ref)
                .and_then(|output| output.prepared_report()["artifacts"].as_array())
                .filter(|artifacts| artifacts.len() == 1)
                .and_then(|artifacts| artifacts.first())
                .ok_or(AdmissionFailure::Incomplete)?;
            visit(Borrowed {
                source: Source::Primary(index),
                node,
                field: "path",
                path: artifact.path(),

                observation,
                specification,
            })?;
        }
    }
    Ok(())
}
impl Publication {
    /// The caller is FileBuild's private actual-build boundary. Paths, bytes,
    /// tables and observations are selected only from its immutable owners.
    pub(crate) fn from_build<E>(
        attempt: &NativeAttempt<E>,
        prepared: Result<PreparedPublication, &'static str>,
        logs: Arc<[NodeLogs]>,
        resources: &Resources,
    ) -> Self {
        let (primary, refusal) = if attempt.accepted() {
            match prepared {
                Ok(primary) => (Some(primary), None),
                Err(code) => (None, Some(code)),
            }
        } else {
            (None, None)
        };
        let requests = Self::collect_requests(attempt, primary.as_ref(), &logs);
        let (requests, full, diagnostic) = match requests {
            Err(error) => (Vec::new(), Err(Admission::unlocated(error)), None),
            Ok(requests) => {
                let full = if let Some(code) = refusal {
                    Err(Admission::unlocated(AdmissionFailure::Rejected(code)))
                } else {
                    Self::targets(&requests, false, &attempt.provenance, resources)
                };
                let diagnostic = full
                    .is_err()
                    .then(|| Self::targets(&requests, true, &attempt.provenance, resources));
                (requests, full, diagnostic)
            }
        };
        Self {
            primary,
            logs,
            requests,
            full,
            diagnostic,
        }
    }
    fn collect_requests<E>(
        attempt: &NativeAttempt<E>,
        primary: Option<&PreparedPublication>,
        logs: &[NodeLogs],
    ) -> Result<Vec<Request>, AdmissionFailure> {
        let nodes = attempt.provenance.metadata().nodes().len();
        if nodes == 0 || logs.len() > nodes {
            return Err(AdmissionFailure::Incomplete);
        }
        for (index, held) in logs.iter().enumerate() {
            if held.node >= nodes || logs[..index].iter().any(|prior| prior.node == held.node) {
                return Err(AdmissionFailure::Incomplete);
            }
        }
        let mut count = 0usize;
        each(attempt, primary, logs, |_| {
            count += 1;
            Ok(())
        })?;
        if count == 0 {
            return Err(AdmissionFailure::Incomplete);
        }
        // Cumulative path, byte, traversal and observation admission precedes
        // all new requests. Exact held byte owners are reused, never reencoded.
        let mut requests = Vec::with_capacity(count);
        each(attempt, primary, logs, |request| {
            let mut observation = request.observation.clone();
            if !observation.is_object() {
                return Err(AdmissionFailure::Incomplete);
            }
            observation["specification"] = Value::String(request.specification.into());
            requests.push(Request {
                source: request.source,
                node: request.node,
                field: request.field,
                path: request.path.into(),
                observation,
            });
            Ok(())
        })?;
        Ok(requests)
    }
    fn targets(
        requests: &[Request],
        diagnostic: bool,
        provenance: &Provenance,
        resources: &Resources,
    ) -> Result<Targets, Admission> {
        let selected = |request: &&Request| !diagnostic || request.field == "verification_log";
        let count = requests.iter().filter(selected).count();
        if count == 0 {
            return Err(Admission::unlocated(AdmissionFailure::Incomplete));
        }
        // Validate every retained metadata owner before native target selection.
        for (index, request) in requests
            .iter()
            .enumerate()
            .filter(|(_, request)| selected(request))
        {
            let view = provenance
                .report_views()
                .nth(request.node)
                .ok_or_else(|| Admission::at(index, AdmissionFailure::Incomplete))?;
            view.document()
                .written_output_field_origin(request.field)
                .ok_or_else(|| Admission::at(index, AdmissionFailure::Incomplete))?;
        }
        let mut targets = Vec::with_capacity(count);
        for (index, request) in requests.iter().enumerate() {
            if diagnostic && request.field != "verification_log" {
                continue;
            }
            let view = provenance.report_views().nth(request.node).unwrap();
            let origin = view
                .document()
                .written_output_field_origin(request.field)
                .unwrap();
            let failure = |error| {
                Admission::at(
                    index,
                    AdmissionFailure::Resource {
                        node: request.node,
                        error,
                    },
                )
            };
            let selected = resources
                .location_from(origin.base_source, origin.written)
                .map_err(failure)?;
            let target = resources
                .publication_target_from(origin.base_source, origin.written)
                .map_err(failure)?;
            if targets
                .iter()
                .any(|prior: &Target| same_location(&prior.selected, &selected))
            {
                return Err(failure(ResourceError::InvalidPath));
            }
            targets.push(Target {
                request: index,
                selected,
                target,
            });
        }
        Ok(Targets { values: targets })
    }
    pub(crate) fn requests(&self, diagnostic: bool) -> impl Iterator<Item = (usize, &Request)> {
        self.requests
            .iter()
            .enumerate()
            .filter(move |(_, request)| !diagnostic || request.field == "verification_log")
    }
    pub(crate) fn request(&self, index: usize) -> &Request {
        &self.requests[index]
    }
    fn bytes(&self, request: &Request) -> &[u8] {
        match request.source {
            Source::Primary(index) => self.primary.as_ref().unwrap().artifacts()[index].bytes(),
            Source::Verification(index) => {
                self.logs[index].logs.verification.as_ref().unwrap().bytes()
            }
            Source::Warning(index) => self.logs[index].logs.warning.as_ref().unwrap().bytes(),
        }
    }
    fn targets_for(&self, diagnostic: bool) -> Result<&Targets, &Admission> {
        if diagnostic {
            if let Some(targets) = &self.diagnostic {
                return targets.as_ref();
            }
        }
        self.full.as_ref()
    }
    pub(crate) fn admission_failure(&self, diagnostic: bool) -> Option<&Admission> {
        self.targets_for(diagnostic).err()
    }
    pub(crate) fn prefix_report(
        &self,
        original: &StoredReport,
        diagnostic: bool,
        entered: usize,
        completed: usize,
    ) -> Result<StoredReport, ReportError> {
        let count = self.requests(diagnostic).count();
        if completed > entered || entered > count {
            return Err(ReportError::InvalidObservation);
        }

        let mut result = original.without_artifacts_cloned()?;
        result["artifacts"] = Value::Array(
            self.requests(diagnostic)
                .take(completed)
                .map(|(_, request)| request.observation.clone())
                .collect(),
        );
        result["publication"] = crate::json!({
            "status":"failure", "mode":if diagnostic {"diagnostic"} else {"complete"},
            "entered":self.requests(diagnostic).take(entered).enumerate()
                .map(|(position, (_, request))| crate::json!({"node":request.node,
                    "field":request.field,"path":request.path,"published":position < completed})).collect::<Vec<_>>(),
            "error":{"stage":"publication","code":"publication_incomplete","node":null,"field":null}
        });
        Ok(result)
    }
    pub(crate) fn save_into(
        &self,
        diagnostic: bool,
        attempt: &mut Attempt,
        mut publish: impl FnMut(
            &mut Publisher,
            &str,
            &[u8],
        ) -> Result<(), crate::file_publication::Error>,
    ) {
        *attempt = Default::default();
        let targets = match self.targets_for(diagnostic) {
            Ok(targets) => targets,
            Err(_) => {
                attempt.outcome = Err(Failure::Admission);
                return;
            }
        };
        let count = self.requests(diagnostic).count();
        // Retain all potential entry text before opening any publisher. The
        // actual ledger receives only entries moved before crossing the port.
        let mut entries = self
            .requests(diagnostic)
            .map(|(index, request)| Entered {
                request: index,
                node: request.node,
                field: request.field,
                path: request.path.clone(),
                published: false,
            })
            .collect::<VecDeque<_>>();
        attempt.entered.reserve(count);
        let mut publishers: Vec<Publisher> = Vec::with_capacity(count);
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            for (index, request) in self.requests(diagnostic) {
                let target = targets
                    .values
                    .iter()
                    .find(|target| target.request == index)
                    .ok_or(Failure::Incomplete)?;
                let failure = |error| {
                    Failure::Preflight(TargetFailure {
                        request: index,
                        error,
                    })
                };
                let publisher = target.target.publisher(&request.path).map_err(failure)?;
                for previous in &publishers {
                    if publisher.same_target(previous).map_err(failure)? {
                        return Err(failure(crate::file_publication::Error::InvalidTarget));
                    }
                }
                publishers.push(publisher);
            }
            for ((index, request), publisher) in self.requests(diagnostic).zip(&mut publishers) {
                attempt
                    .entered
                    .push(entries.pop_front().expect("admitted entry"));
                publish(publisher, &request.path, self.bytes(request)).map_err(|error| {
                    Failure::Publish(TargetFailure {
                        request: index,
                        error,
                    })
                })?;
                attempt.entered.last_mut().unwrap().published = true;
            }
            Ok(())
        }));
        match outcome {
            Ok(result) => attempt.outcome = result,
            Err(payload) => attempt.unwind = Some(payload),
        }
    }
}
