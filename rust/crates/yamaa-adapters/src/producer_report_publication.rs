//! Report and primary publication sealed beside one actual native build.
//! Failed graphs retain report-only authority; public attempts grant none.
use crate::{
    file_producer_build::Provenance,
    file_resources::{Error as ResourceError, Resources},
    issue_rows::Issue,
    producer_publication::{same_location, AdmissionFailure},
    producer_report::{NativeAttempt, NodeRunReport},
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

/// Every output file of the complete authored graph names its own location:
/// two primary files, two reports, or a report and a primary file of any nodes
/// that resolve to one location are metadata facts, refused before any study
/// read whether or not a node declares a report. The comparison needs neither
/// file nor parent to exist; physical alias checks remain at Save's whole-set
/// preflight.
pub(crate) fn admit_locations(
    documents: &[Arc<crate::specification_source::PreparedDocument>],
    resources: &Resources,
) -> Result<(), crate::file_graph::Error> {
    use crate::file_graph::Error;
    use yamaa_core::specification::{PreflightFinding, PrepareError, ReportFinding};
    let fields = [("path", "output.path"), ("report", "output.report")];
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
                    let finding = PreflightFinding::Report(ReportFinding::GraphCollision {
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
    /// The report of the request's node, held by the result.
    Report,
}
pub(crate) struct Request {
    source: Source,
    pub(crate) node: usize,
    pub(crate) field: &'static str,
    pub(crate) path: String,
    specification: String,
    /// A primary artifact's complete observation. A report's observation is
    /// held with its rendered bytes by the result that built the report.
    observation: Option<Value>,
}
struct Borrowed<'a> {
    source: Source,
    node: usize,
    field: &'static str,
    path: &'a str,
    observation: Option<&'a Value>,
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
    requests: Vec<Request>,
    full: Result<Targets, Admission>,
    // A rejected primary target cannot consume a retained report capability.
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

/// Visit the graph's files in node order: each node that ran and declares
/// `output.report` publishes its report immediately before its own primary
/// artifact, and an accepted graph publishes every primary artifact.
fn each<E>(
    attempt: &NativeAttempt<E>,
    primary: Option<&PreparedPublication>,
    mut visit: impl FnMut(Borrowed<'_>) -> Result<(), AdmissionFailure>,
) -> Result<(), AdmissionFailure> {
    for &node in attempt.provenance.metadata().order() {
        let specification = attempt.provenance.metadata().nodes()[node].identity();
        let view = attempt
            .provenance
            .report_views()
            .nth(node)
            .ok_or(AdmissionFailure::Incomplete)?;
        // A node that never ran built no report.
        let ran = attempt
            .graph
            .nodes
            .iter()
            .any(|entered| entered.node == node);
        if let Some(path) = view.compiled().output_report().filter(|_| ran) {
            visit(Borrowed {
                source: Source::Report,
                node,
                field: "report",
                path,
                observation: None,
                specification,
            })?;
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

                observation: Some(observation),
                specification,
            })?;
        }
    }
    Ok(())
}
impl Publication {
    /// The caller is FileBuild's private actual-build boundary. Paths, primary
    /// bytes and observations are selected only from its immutable owners; the
    /// reports are built, rendered and verified with the result that holds
    /// them, before any Save.
    pub(crate) fn from_build<E>(
        attempt: &NativeAttempt<E>,
        prepared: Result<PreparedPublication, &'static str>,
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
        let requests = Self::collect_requests(attempt, primary.as_ref());
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
            requests,
            full,
            diagnostic,
        }
    }
    fn collect_requests<E>(
        attempt: &NativeAttempt<E>,
        primary: Option<&PreparedPublication>,
    ) -> Result<Vec<Request>, AdmissionFailure> {
        if attempt.provenance.metadata().nodes().is_empty() {
            return Err(AdmissionFailure::Incomplete);
        }
        let mut count = 0usize;
        each(attempt, primary, |_| {
            count += 1;
            Ok(())
        })?;
        if count == 0 {
            return Err(AdmissionFailure::Incomplete);
        }
        // Cumulative path, traversal and primary observation admission precedes
        // all new requests. Exact held byte owners are reused, never reencoded.
        let mut requests = Vec::with_capacity(count);
        each(attempt, primary, |request| {
            let observation = request
                .observation
                .map(|observation| {
                    let mut observation = observation.clone();
                    if !observation.is_object() {
                        return Err(AdmissionFailure::Incomplete);
                    }
                    observation["specification"] = Value::String(request.specification.into());
                    Ok(observation)
                })
                .transpose()?;
            requests.push(Request {
                source: request.source,
                node: request.node,
                field: request.field,
                path: request.path.into(),
                specification: request.specification.into(),
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
        let selected = |request: &&Request| !diagnostic || request.field == "report";
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
            if diagnostic && request.field != "report" {
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
            .filter(move |(_, request)| !diagnostic || request.field == "report")
    }
    pub(crate) fn request(&self, index: usize) -> &Request {
        &self.requests[index]
    }
    /// The rendered report file the result holds for a report request.
    fn held<'a>(request: &Request, reports: &'a [NodeRunReport]) -> Option<(&'a [u8], &'a Value)> {
        reports
            .iter()
            .find(|held| held.node == request.node)
            .and_then(|held| held.report.file())
            .filter(|(path, _, _)| *path == request.path)
            .map(|(_, bytes, observation)| (bytes, observation))
    }
    fn bytes<'a>(&'a self, request: &Request, reports: &'a [NodeRunReport]) -> Option<&'a [u8]> {
        match request.source {
            Source::Primary(index) => self
                .primary
                .as_ref()
                .and_then(|primary| primary.artifacts().get(index))
                .map(|artifact| artifact.bytes()),
            Source::Report => Self::held(request, reports).map(|(bytes, _)| bytes),
        }
    }
    fn observation(request: &Request, reports: &[NodeRunReport]) -> Result<Value, ReportError> {
        match request.source {
            Source::Primary(_) => request
                .observation
                .clone()
                .ok_or(ReportError::InvalidObservation),
            Source::Report => {
                let (_, observation) =
                    Self::held(request, reports).ok_or(ReportError::InvalidObservation)?;
                let mut observation = observation.clone();
                if !observation.is_object() {
                    return Err(ReportError::InvalidObservation);
                }
                observation["specification"] = Value::String(request.specification.clone());
                Ok(observation)
            }
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
    /// Open the publisher of every request a save in this mode would enter,
    /// without writing; the caller checks them against other results' files.
    pub(crate) fn publishers(&self, diagnostic: bool) -> Result<Vec<Publisher>, Vec<Issue>> {
        let targets =
            self.targets_for(diagnostic)
                .map_err(|admission| match &admission.failure {
                    AdmissionFailure::Resource { error, .. } => {
                        crate::file_application::resource_failure("output", error)
                    }
                    AdmissionFailure::Rejected(code) => {
                        crate::file_application::rejected("output", code)
                    }
                    AdmissionFailure::Incomplete => {
                        crate::file_application::rejected("output", "publication_incomplete")
                    }
                })?;
        self.requests(diagnostic)
            .map(|(index, request)| {
                let target = targets
                    .values
                    .iter()
                    .find(|target| target.request == index)
                    .ok_or_else(|| {
                        crate::file_application::rejected("output", "publication_incomplete")
                    })?;
                target
                    .target
                    .publisher(&request.path)
                    .map_err(|error| crate::file_application::publication_failure(&error))
            })
            .collect()
    }
    pub(crate) fn prefix_report(
        &self,
        original: &StoredReport,
        reports: &[NodeRunReport],
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
                .map(|(_, request)| Self::observation(request, reports))
                .collect::<Result<_, _>>()?,
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
        reports: &[NodeRunReport],
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
        // Save writes only bytes the result already holds: every selected
        // file is owned before the first publisher opens.
        if self
            .requests(diagnostic)
            .any(|(_, request)| self.bytes(request, reports).is_none())
        {
            attempt.outcome = Err(Failure::Incomplete);
            return;
        }
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
                let bytes = self
                    .bytes(request, reports)
                    .expect("held before publication");
                publish(publisher, &request.path, bytes).map_err(|error| {
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
