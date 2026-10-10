//! Complete reports borrow the sealed whole graph and actual entered attempts.
//! Rendering and obtaining evidence grant no publication or callback authority.
use crate::{
    file_producer_build::{NodeReport, Provenance},
    project_report,
    report_projection::Budget,
    specification_report::{self as report, Error, Identity},
};
use serde_json::{json, Value};
use yamaa_core::{producer_graph::Node, table::TableAccess};
use yamaa_engine::{
    dataset::{CheckRecord, Execution},
    producer_build::ReportPort,
    specification_output::OutputReport,
    specification_run::CapturedAttempt,
};

fn records(
    budget: &mut Budget,
    view: &NodeReport<'_>,
    records: &[CheckRecord],
) -> Result<(), Error> {
    budget.entries(records.len())?;
    let keys = view.compiled().key_names().count();
    budget.entries(keys)?;
    // Check projection consults the retained compiler's ordered declarations.
    // Bound repeated lookups independently of the resulting JSON byte count.
    budget.work(
        records
            .len()
            .checked_mul(view.document().normalization_input().nodes().len())
            .and_then(|n| n.checked_mul(16))
            .ok_or(Error::OutputLimit)?,
    )?;
    for record in records {
        budget.work(
            record
                .path
                .len()
                .checked_mul(view.document().normalization_input().nodes().len())
                .and_then(|n| n.checked_mul(16))
                .ok_or(Error::OutputLimit)?,
        )?;
        budget.entries(64)?;
        for _ in 0..8 {
            for column in view.compiled().output().columns() {
                budget.text(&column.name)?;
            }
        }
        for _ in 0..8 {
            budget.text(view.node().identity())?;
            if let Some(target) = view.compiled().verification_target(&record.path) {
                budget.text(target)?;
            }
            if let Some(id) = view.compiled().verification_identity(&record.path) {
                budget.text(id)?;
            }
            for name in view.compiled().key_names() {
                budget.text(name)?;
            }
        }
        if record.failed_count > 0 {
            if let Some(check) = view.compiled().column_check(&record.path) {
                budget.check(check)?;
            }
        }
        for text in [&record.path, record.condition, record.requirement] {
            for _ in 0..8 {
                budget.text(text)?;
            }
        }
        budget.entries(record.offending_rows.len())?;
        for row in &record.offending_rows {
            budget.entries(row.values.len())?;
            for _ in 0..8 {
                for name in view.compiled().key_names() {
                    budget.text(name)?;
                }
            }
            for value in &row.values {
                for _ in 0..8 {
                    budget.scalar(value)?;
                }
            }
        }
        if let Some(codelist) = &record.codelist {
            budget.text(&codelist.id)?;
            budget.entries(codelist.values.len())?;
            for value in &codelist.values {
                for _ in 0..8 {
                    budget.scalar(value)?;
                }
            }
        }
    }
    Ok(())
}
fn identity<'a>(view: &'a NodeReport<'_>, id: &'a Identity<'_>) -> Identity<'a> {
    let specification = view.node().identity();
    let base_directory =
        specification.rsplit_once('/').map_or(
            ".",
            |(base, _)| {
                if base.is_empty() {
                    "/"
                } else {
                    base
                }
            },
        );
    Identity {
        runtime: id.runtime,
        runtime_version: id.runtime_version,
        engine_version: id.engine_version,
        example: id.example,
        specification,
        base_directory,
    }
}
fn successful_observations<C, D, T: TableAccess>(
    view: &NodeReport<'_>,
    attempt: &CapturedAttempt<C, D, T>,
    id: &Identity<'_>,
    budget: &mut Budget,
) -> Result<Value, Error> {
    let entered = attempt
        .result
        .as_ref()
        .map_err(|_| Error::InvalidObservation)?;
    let success = entered
        .result
        .as_ref()
        .map_err(|_| Error::InvalidObservation)?;
    if attempt.sources.len() != view.compiled().sources().len()
        || attempt
            .sources
            .iter()
            .zip(view.compiled().sources())
            .any(|(held, declared)| {
                held.read.source != *declared
                    || !held.read.captured
                    || held.read.failure.is_some()
                    || held.snapshot.is_none()
                    || held.table.is_none()
            })
    {
        return Err(Error::InvalidObservation);
    }
    records(budget, view, &entered.retained_verifications)?;
    records(budget, view, &success.verifications)?;
    budget.entries(entered.handler_counts.len())?;
    for count in &entered.handler_counts {
        budget.entries(8)?;
        budget.text(&count.spec_path)?;
        budget.text(&count.spec_path)?;
    }
    let mut value = report::envelope_sources_projected(view, &attempt.sources, id, Some(budget))?;
    let (mut checks, findings) =
        project_report::checked_records(view, &entered.retained_verifications, "verification", id)?;
    if !findings.is_empty() {
        return Err(Error::InvalidObservation);
    }
    let (tail, findings) =
        project_report::checked_records(view, &success.verifications, "verification", id)?;
    if !findings.is_empty() {
        return Err(Error::InvalidObservation);
    }
    checks.extend(tail);
    value["verifications"] = json!(checks);
    report::set_handler_counts(
        &mut value,
        &json!({"handler_counts":entered.handler_counts.iter().map(|count|
        json!({"spec_path":count.spec_path,"handler":count.handler.name(),"count":count.count.to_string()})
    ).collect::<Vec<_>>()}),
    )?;
    Ok(value)
}

/// Native successful node reports are completed inside the build, before the
/// artifact becomes available to consumers. Each selection starts fresh state.
pub struct NativeReport<'a, 'i> {
    provenance: &'a Provenance,
    id: Identity<'i>,
    selected: Option<usize>,
    maximum: usize,
    budget: Option<Budget>,
    value: Option<Value>,
}
impl<'a, 'i> NativeReport<'a, 'i> {
    pub fn new(provenance: &'a Provenance, id: Identity<'i>) -> Self {
        Self {
            provenance,
            id,
            selected: None,
            maximum: 0,
            budget: None,
            value: None,
        }
    }
    fn view(&self) -> Result<NodeReport<'a>, Error> {
        self.provenance
            .report_views()
            .nth(self.selected.ok_or(Error::InvalidObservation)?)
            .ok_or(Error::InvalidObservation)
    }
    fn finish(&self, value: Value) -> Result<Value, Error> {
        if value.to_string().len() > self.maximum {
            return Err(Error::OutputLimit);
        }
        report::bounded(value)
    }
}
impl ReportPort for NativeReport<'_, '_> {
    fn select<C, D, T: TableAccess>(
        &mut self,
        node: &Node,
        attempt: &CapturedAttempt<C, D, T>,
        maximum: usize,
    ) -> Result<(), Error> {
        self.selected = None;
        self.budget = None;
        self.value = None;
        self.maximum = maximum;
        let mut budget = Budget::new(maximum);
        let count = self.provenance.metadata().nodes().len();
        budget.entries(count)?;
        budget.work(count.checked_mul(3).ok_or(Error::OutputLimit)?)?;
        let index = self
            .provenance
            .metadata()
            .nodes()
            .iter()
            .position(|held| std::ptr::eq(held, node))
            .ok_or(Error::InvalidObservation)?;
        let view = self
            .provenance
            .report_views()
            .nth(index)
            .ok_or(Error::InvalidObservation)?;
        let value =
            successful_observations(&view, attempt, &identity(&view, &self.id), &mut budget)?;
        self.selected = Some(index);
        self.budget = Some(budget);
        self.value = Some(value);
        Ok(())
    }
}
impl OutputReport for NativeReport<'_, '_> {
    type Error = Error;
    type Report = Value;
    fn failure(&mut self) -> Result<Value, Error> {
        let value = self.value.take().ok_or(Error::InvalidObservation)?;
        self.finish(value)
    }
    fn begin(&mut self, _execution: &Execution) -> Result<(), Error> {
        self.value.as_ref().ok_or(Error::InvalidObservation)?;
        Ok(())
    }
    fn rejected(
        &mut self,
        findings: &[yamaa_core::specification::OutputFinding],
    ) -> Result<Value, Error> {
        let budget = self.budget.as_mut().ok_or(Error::InvalidObservation)?;
        budget.entries(findings.len())?;
        for finding in findings {
            use yamaa_core::specification::OutputFinding::*;
            budget.entries(32)?;
            let text = match finding {
                UnknownProfile { path } | DecimalsNotApplicable { path } => path,
                InvalidDecimals { value } => value,
                DuplicateColumn { name, .. }
                | UndeclaredColumn { name, .. }
                | InternalKey { name, .. } => name,
            };
            for _ in 0..4 {
                budget.text(text)?;
            }
        }
        let diagnostics = report::output_diagnostics(findings)?;
        let mut value = self.value.take().ok_or(Error::InvalidObservation)?;
        report::set_diagnostics(&mut value, diagnostics);
        self.finish(value)
    }
    fn success(
        &mut self,
        execution: &Execution,
        projection: &[usize],
        bytes: &[u8],
    ) -> Result<Value, Error> {
        let view = self.view()?;
        let value = self.value.take().ok_or(Error::InvalidObservation)?;
        let budget = self.budget.as_mut().ok_or(Error::InvalidObservation)?;
        let value = report::artifact_report(
            &view,
            &identity(&view, &self.id),
            value,
            execution,
            projection,
            bytes,
            Some(budget),
        )?;
        self.finish(value)
    }
}

pub type NativeAttempt<E> = crate::file_producer_build::Attempt<E, Value, Error>;

/// Projection refusal borrows the exact complete native attempt. The bounded
/// actual prefix remains available beside opaque host failures and interrupts.
/// Neither variant can be saved or passed to a publisher.
pub enum ProjectionError<'a, E> {
    Report(Error),
    Original {
        attempt: &'a NativeAttempt<E>,
        observations: Value,
    },
}
impl<E> std::fmt::Debug for ProjectionError<'_, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Report(error) => f.debug_tuple("Report").field(error).finish(),
            Self::Original { .. } => f.write_str("Original(..)"),
        }
    }
}
impl<E> From<Error> for ProjectionError<'_, E> {
    fn from(error: Error) -> Self {
        Self::Report(error)
    }
}
fn activation_prefix<E>(attempt: &NativeAttempt<E>, budget: &mut Budget) -> Result<Value, Error> {
    use crate::project_activation_observations::{self as activation, ActivationView};
    use yamaa_engine::producer_build::Failure;
    let held = attempt.provenance.as_ref();
    let observed = &attempt.graph.activation;
    let maximum = budget.remaining_bytes();
    budget.entries(32)?;
    budget.entries(observed.bindings.len())?;
    budget.entries(observed.tests.len())?;
    let lookups = observed
        .bindings
        .len()
        .checked_add(observed.tests.len())
        .ok_or(Error::OutputLimit)?;
    budget.work(
        lookups
            .checked_mul(held.environment().captures().len())
            .and_then(|n| n.checked_mul(2))
            .ok_or(Error::OutputLimit)?,
    )?;
    for binding in &observed.bindings {
        budget.entries(16)?;
        let index = *held
            .called_functions()
            .get(binding.function)
            .ok_or(Error::InvalidObservation)?;
        let function = held
            .metadata()
            .environment()
            .functions()
            .get(index)
            .ok_or(Error::InvalidObservation)?;
        budget.text(&function.definition().name)?;
        budget.text(&function.definition().function)?;
    }
    for case in &observed.tests {
        budget.entries(32)?;
        let index = *held
            .called_functions()
            .get(case.function)
            .ok_or(Error::InvalidObservation)?;
        let function = held
            .metadata()
            .environment()
            .functions()
            .get(index)
            .ok_or(Error::InvalidObservation)?;
        let definition = function.definition();
        let declared = definition
            .tests
            .get(case.case)
            .ok_or(Error::InvalidObservation)?;
        for text in [&definition.name, &definition.function, &declared.id] {
            budget.text(text)?;
        }
        budget.entries(declared.args.len())?;
        for (name, value) in &declared.args {
            budget.text(name)?;
            budget.scalar(value)?;
        }
        budget.scalar(&declared.result)?;
        if let Some(value) = &case.actual {
            budget.scalar(value)?;
        }
    }
    // A source identity is copied for every observed binding/case. Admit its
    // complete retained origin scan before the existing prefix formatter grows.
    for _ in 0..lookups {
        budget.text(&held.environment().root().source().identity)?;
        for capture in held.environment().captures() {
            budget.text(&capture.document.source().identity)?;
        }
    }
    let value =
        activation::activation_with_limit(held, observed, maximum).map_err(
            |error| match error {
                activation::Error::Limit => Error::OutputLimit,
                _ => Error::InvalidObservation,
            },
        )?;
    let boundary = match &attempt.graph.outcome {
        Ok(()) | Err(Failure::Node { .. } | Failure::Binding { .. }) => {
            project_report::ActivationBoundary::Complete
        }
        Err(Failure::Activation(error)) => project_report::ActivationBoundary::Failed(error),
        _ => project_report::ActivationBoundary::Incomplete,
    };
    project_report::validate_activation(held, observed, boundary)?;
    Ok(value)
}
fn push_diagnostic(
    budget: &mut Budget,
    findings: &mut Vec<Value>,
    diagnostic: &yamaa_core::diagnostic::Diagnostic,
) -> Result<(), Error> {
    // A diagnostic occurs in both the aggregate and the original node finding.
    for _ in 0..4 {
        budget.diagnostic(diagnostic)?;
    }
    findings.push(
        crate::specification_diagnostics::portable_diagnostic(diagnostic.clone())
            .ok_or(Error::InvalidObservation)?,
    );
    Ok(())
}
fn source_finding(
    budget: &mut Budget,
    findings: &mut Vec<Value>,
    view: &NodeReport<'_>,
    source: &yamaa_core::specification::SourceDeclaration,
    failure: yamaa_core::resource::ResourceFailure,
) -> Result<(), Error> {
    let written = view
        .document()
        .written_source_path(&source.name)
        .ok_or(Error::InvalidObservation)?;
    budget.entries(32)?;
    for _ in 0..4 {
        budget.text(&source.name)?;
        budget.text(written)?;
    }
    push_diagnostic(budget, findings, &failure.diagnostic(&source.name, written))
}
fn native_resource_failure(
    error: &yamaa_engine::producer_build::CaptureError<crate::file_resources::Error>,
) -> Option<yamaa_core::resource::ResourceFailure> {
    use crate::file_resources::Error as Native;
    use yamaa_core::resource::ResourceFailure;
    match error {
        yamaa_engine::producer_build::CaptureError::External(Native::Missing) => {
            Some(ResourceFailure::Missing)
        }
        yamaa_engine::producer_build::CaptureError::External(Native::NotRegularFile) => {
            Some(ResourceFailure::NotRegularFile)
        }
        _ => None,
    }
}
fn decoded_finding(
    budget: &mut Budget,
    findings: &mut Vec<Value>,
    source: &yamaa_core::specification::SourceDeclaration,
    error: &yamaa_engine::producer_build::DecodeError<crate::specification_run::Error>,
) -> Result<bool, Error> {
    use yamaa_engine::producer_build::DecodeError;
    if let DecodeError::Metadata(diagnostic) = error {
        push_diagnostic(budget, findings, diagnostic)?;
        return Ok(true);
    }
    let DecodeError::Codec(error) = error else {
        return Ok(false);
    };
    for _ in 0..4 {
        budget.text(&source.name)?;
        budget.text(&source.path)?;
    }
    budget.entries(64)?;
    match error {
        crate::specification_run::Error::Source(crate::csv_source::TextTableError::Csv(_)) => {}
        crate::specification_run::Error::TypedSource(crate::typed_csv::Error::Typing(error)) => {
            use yamaa_core::typed_csv::Error::*;
            match error {
                UnknownField { field } => {
                    for _ in 0..4 {
                        budget.text(field)?;
                    }
                }
                FieldParse { field, value, .. } => {
                    for _ in 0..4 {
                        budget.text(field)?;
                        budget.text(value)?;
                    }
                }
                _ => return Ok(false),
            }
        }
        crate::specification_run::Error::ParquetSource(error) => {
            use crate::parquet_source::Error::*;
            match error {
                Malformed | EmptyName { .. } => {}
                DuplicateName { field } | Value { field, .. } => {
                    for _ in 0..4 {
                        budget.text(field)?;
                    }
                }
                Unsupported { field, stored_type } => {
                    for _ in 0..4 {
                        budget.text(field)?;
                        budget.text(stored_type)?;
                    }
                }
                _ => return Ok(false),
            }
        }
        _ => return Ok(false),
    }
    let Some(values) = crate::specification_diagnostics::findings(error, Some(source)) else {
        return Ok(false);
    };
    findings.extend(values);
    Ok(true)
}
fn failed_observations<E>(
    view: &NodeReport<'_>,
    entered: &crate::file_producer_build::GraphAttempt<E, Value, Error>,
    position: usize,
    id: &Identity<'_>,
    budget: &mut Budget,
) -> Result<(Value, bool), Error> {
    use yamaa_engine::dataset::ExecutionError;
    use yamaa_engine::specification_run::{PortError, RunError};
    let node = &entered.nodes[position];
    let attempt = &node.dataset;
    if attempt.sources.len() > view.compiled().sources().len()
        || attempt
            .sources
            .iter()
            .zip(view.compiled().sources())
            .any(|(held, expected)| held.read.source != *expected)
    {
        return Err(Error::InvalidObservation);
    }
    let mut value = report::envelope_sources_projected(view, &attempt.sources, id, Some(budget))?;
    let mut diagnostics = Vec::new();
    let mut verifications = Vec::new();
    let mut portable = true;
    match &attempt.result {
        Ok(execution) => {
            records(budget, view, &execution.retained_verifications)?;
            let (checks, findings) = project_report::checked_records(
                view,
                &execution.retained_verifications,
                "verification",
                id,
            )?;
            verifications.extend(checks);
            diagnostics.extend(findings);
            budget.entries(execution.handler_counts.len())?;
            for count in &execution.handler_counts {
                budget.entries(16)?;
                for _ in 0..4 {
                    budget.text(&count.spec_path)?;
                }
            }
            report::set_handler_counts(
                &mut value,
                &json!({"handler_counts":execution.handler_counts.iter().map(|count| json!({"spec_path":count.spec_path,"handler":count.handler.name(),"count":count.count.to_string()})).collect::<Vec<_>>()}),
            )?;
            let tail: &[CheckRecord] = match &execution.result {
                Ok(success) => &success.verifications[..],
                Err(error) => match error.as_ref() {
                    ExecutionError::VerificationFailures(records)
                    | ExecutionError::KeyFailures(records) => records,
                    ExecutionError::VerificationDiagnostic {
                        records,
                        diagnostic,
                    } => {
                        push_diagnostic(budget, &mut diagnostics, diagnostic)?;
                        records
                    }
                    ExecutionError::VerificationDeclaration {
                        path,
                        condition,
                        requirement,
                        reason,
                        records,
                    } => {
                        for _ in 0..4 {
                            budget.text(path)?;
                            budget.text(reason)?;
                        }
                        let diagnostic = yamaa_core::dataset_checks::declaration_diagnostic(
                            path.clone(),
                            condition,
                            requirement,
                            reason.clone(),
                        )
                        .ok_or(Error::InvalidObservation)?;
                        push_diagnostic(budget, &mut diagnostics, &diagnostic)?;
                        records
                    }
                    _ => {
                        portable = false;
                        &[]
                    }
                },
            };
            records(budget, view, tail)?;
            let phase = if matches!(&execution.result, Err(error) if matches!(error.as_ref(), ExecutionError::KeyFailures(_)))
            {
                "output"
            } else {
                "verification"
            };
            let (checks, findings) = project_report::checked_records(view, tail, phase, id)?;
            verifications.extend(checks);
            diagnostics.extend(findings);
        }
        Err(PortError::Inspect(failures)) => {
            if !attempt.sources.is_empty() || failures.is_empty() {
                return Err(Error::InvalidObservation);
            }
            budget.entries(failures.len())?;
            let mut sources = view.compiled().sources().iter();
            for failure in failures {
                if !sources.any(|source| source == &failure.source) {
                    return Err(Error::InvalidObservation);
                }
                if failure.failure != native_resource_failure(&failure.error) {
                    return Err(Error::InvalidObservation);
                }
                if let Some(cause) = failure.failure {
                    source_finding(budget, &mut diagnostics, view, &failure.source, cause)?;
                } else {
                    portable = false;
                }
            }
        }
        Err(PortError::Capture(error)) => {
            let last = attempt.sources.last().ok_or(Error::InvalidObservation)?;
            if last.read.captured || last.snapshot.is_some() || last.table.is_some() {
                return Err(Error::InvalidObservation);
            }
            if last.read.failure != native_resource_failure(error) {
                return Err(Error::InvalidObservation);
            }
            if let Some(cause) = last.read.failure {
                source_finding(budget, &mut diagnostics, view, &last.read.source, cause)?;
            } else {
                portable = false;
            }
        }
        Err(PortError::Run(RunError::Decode(error))) => {
            portable &= decoded_finding(budget, &mut diagnostics, view.compiled().source(), error)?
        }
        Err(PortError::Run(RunError::Sources(errors))) => {
            budget.entries(errors.len())?;
            for (index, error) in errors {
                let source = view
                    .compiled()
                    .sources()
                    .get(*index)
                    .ok_or(Error::InvalidObservation)?;
                portable &= decoded_finding(budget, &mut diagnostics, source, error)?;
            }
        }
        _ => portable = false,
    }
    match &node.output {
        Err(yamaa_engine::specification_output::CompleteError::Report(error)) => {
            budget.entries(16)?;
            value["error"] = json!({"stage":"report","code":match error { Error::OutputLimit => "output_limit", _ => "projection_refused" }});
        }
        Err(_) => portable = false,
        Ok(Some(output)) if output.artifact().is_none() => {
            budget.json(output.prepared_report())?;
            diagnostics.extend(
                output.prepared_report()["diagnostics"]
                    .as_array()
                    .ok_or(Error::InvalidObservation)?
                    .iter()
                    .cloned(),
            );
        }
        _ => {}
    }
    report::set_diagnostics(&mut value, diagnostics);
    value["verifications"] = json!(verifications);
    Ok((value, portable))
}
/// Read the exact retained union and entered prefix. Successful reports reuse the
/// report already accepted before producer ingestion; rendering does not encode,
/// execute, activate, read a file or construct a publication capability.
pub fn attempt_report<'a, E>(
    attempt: &'a NativeAttempt<E>,
    id: Identity<'_>,
    maximum: usize,
) -> Result<Value, ProjectionError<'a, E>> {
    let mut budget = Budget::new(maximum);
    let metadata = attempt.provenance.metadata();
    let entered = &attempt.graph.nodes;
    if entered.len() > metadata.order().len()
        || entered
            .iter()
            .zip(metadata.order())
            .any(|(node, expected)| node.node != *expected)
    {
        return Err(Error::InvalidObservation.into());
    }
    use yamaa_engine::producer_build::Failure;
    match &attempt.graph.outcome {
        Ok(()) if entered.len() != metadata.order().len() => {
            return Err(Error::InvalidObservation.into())
        }
        Err(Failure::Node { node }) => {
            let last = entered.last().ok_or(Error::InvalidObservation)?;
            if last.node != *node
                || last
                    .output
                    .as_ref()
                    .ok()
                    .and_then(Option::as_ref)
                    .and_then(|output| output.artifact())
                    .is_some()
            {
                return Err(Error::InvalidObservation.into());
            }
        }
        Err(Failure::Activation(_) | Failure::Binding { .. } | Failure::Limit(_))
            if !entered.is_empty() =>
        {
            return Err(Error::InvalidObservation.into())
        }
        _ => {}
    }
    budget.entries(64)?;
    budget.entries(entered.len())?;
    for text in [
        id.runtime,
        id.runtime_version,
        id.engine_version,
        id.example,
        attempt.provenance.entry(),
    ] {
        budget.text(text)?;
    }
    let activation = activation_prefix(attempt, &mut budget)?;
    let environment = attempt.provenance.environment();
    budget.entries(8)?;
    budget.text(&environment.root().source().identity)?;
    if let Some(lock) = environment.lock() {
        budget.text(&lock.source.identity)?;
    }
    let environment = json!({"source":environment.root().source().identity,"lock_source":environment.lock().map(|lock| &lock.source.identity)});
    let mut result = json!({"report_version":"0.3.0-draft","runtime":id.runtime,"backend":"rust","runtime_version":id.runtime_version,"engine_version":id.engine_version,"example":id.example,"entry":attempt.provenance.entry(),"outcome":if attempt.accepted() {"success"} else {"failure"},"activation":activation,"environment":environment,"nodes":[],"artifacts":[],"diagnostics":[],"unsupported":[],"handler_counts":[],"tables":[],"verifications":[],"callbacks":[],"source_reads":[],"error":null});
    let mut portable = attempt.boundary.is_ok()
        && attempt.resources.is_ok()
        && !matches!(
            attempt.graph.outcome,
            Err(yamaa_engine::producer_build::Failure::Activation(_)
                | yamaa_engine::producer_build::Failure::Incomplete)
        );
    match &attempt.graph.outcome {
        Err(yamaa_engine::producer_build::Failure::Limit(code)) => {
            budget.entries(8)?;
            budget.text(code)?;
            result["error"] = json!({"stage":"producer_build","code":code});
        }
        Err(yamaa_engine::producer_build::Failure::Binding { node, slot }) => {
            budget.entries(8)?;
            result["error"] = json!({"stage":"producer_binding","node":node,"slot":slot});
        }
        _ => {}
    }
    for (position, node) in entered.iter().enumerate() {
        budget.work(node.node.checked_add(1).ok_or(Error::OutputLimit)?)?;
        let view = attempt
            .provenance
            .report_views()
            .nth(node.node)
            .ok_or(Error::InvalidObservation)?;
        let complete = node
            .output
            .as_ref()
            .ok()
            .and_then(Option::as_ref)
            .filter(|output| output.artifact().is_some());
        let mut value = if let Some(output) = complete {
            // Validate complete source/decode/execution geometry before trusting
            // the retained report. Native report selection already ran its gates.
            let execution = node
                .dataset
                .result
                .as_ref()
                .map_err(|_| Error::InvalidObservation)?;
            if execution.result.is_err()
                || node.dataset.sources.len() != view.compiled().sources().len()
                || node
                    .dataset
                    .sources
                    .iter()
                    .zip(view.compiled().sources())
                    .any(|(source, expected)| {
                        source.read.source != *expected
                            || !source.read.captured
                            || source.snapshot.is_none()
                            || source.table.is_none()
                            || source.read.failure.is_some()
                    })
            {
                return Err(Error::InvalidObservation.into());
            }
            budget.json(output.prepared_report())?;
            output.prepared_report().clone()
        } else {
            if position + 1 != entered.len() {
                return Err(Error::InvalidObservation.into());
            }
            let (value, supported) = failed_observations(
                &view,
                &attempt.graph,
                position,
                &identity(&view, &id),
                &mut budget,
            )?;
            portable &= supported;
            value
        };
        for key in [
            "nodes",
            "artifacts",
            "diagnostics",
            "unsupported",
            "handler_counts",
            "tables",
            "verifications",
            "callbacks",
            "source_reads",
        ] {
            let values = value[key].as_array_mut().ok_or(Error::InvalidObservation)?;
            if key == "source_reads" || key == "artifacts" {
                for item in values.iter_mut() {
                    budget.text(view.node().identity())?;
                    item["specification"] = json!(view.node().identity());
                }
            }
            result[key]
                .as_array_mut()
                .ok_or(Error::InvalidObservation)?
                .append(values);
        }
        if !value["error"].is_null() {
            result["error"] = value["error"].take();
        }
    }
    if attempt.accepted() && entered.len() != metadata.order().len() {
        return Err(Error::InvalidObservation.into());
    }
    if result.to_string().len() > maximum {
        return Err(Error::OutputLimit.into());
    }
    let result = report::bounded(result)?;
    if portable {
        Ok(result)
    } else {
        Err(ProjectionError::Original {
            attempt,
            observations: result,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_header_mismatch_reporting_keeps_the_actual_authored_cause_and_refuses_before_copy() {
        use crate::{project_source_decoder::Decoder, specification_source::Source};
        use yamaa_core::{
            specification::{SourceDeclaration, SourceProfile},
            value::ColumnType,
        };
        use yamaa_engine::producer_build::{DecodeLimits, DecodePort};
        let document = crate::shipped_schema::capture().unwrap().prepare_standalone(Source {
            identity: "original-producer.yaml".into(),
            bytes: b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {RAW: raw.csv}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: RAW.ID}\n  - {name: VALUE, type: int, label: Value, derivation: RAW.VALUE}\noutput: {path: produced.csv, columns: [ID, VALUE]}\n".to_vec(),
        }).unwrap();
        let contract =
            yamaa_core::producer_contract::prepare(document.model(), Default::default()).unwrap();
        let source = SourceDeclaration {
            name: "P".into(),
            path: "produced.csv".into(),
            types: vec![
                ("ID".into(), ColumnType::Int),
                ("VALUE".into(), ColumnType::Int),
            ],
            profile: SourceProfile::Csv,
            empty_string_present: false,
        };
        let bytes = b"VALUE,ID\nnot-an-int,also-bad\n";
        let error = Decoder::<std::convert::Infallible>::default()
            .decode_bounded(
                &source,
                bytes,
                Some(&contract),
                DecodeLimits {
                    cells: 100,
                    storage_bytes: 4096,
                    work_bytes: 4096,
                },
            )
            .err()
            .unwrap();
        let mut findings = vec![];
        assert!(matches!(
            decoded_finding(&mut Budget::new(1), &mut findings, &source, &error),
            Err(Error::OutputLimit)
        ));
        assert!(findings.is_empty());
        assert!(decoded_finding(&mut Budget::new(65536), &mut findings, &source, &error).unwrap());
        assert_eq!(
            findings,
            vec![
                json!({"phase":"validation","condition":"producer_contract_mismatch","requirement":"REQ-0535","spec_paths":["input.P.schema","input.P.path"],"context":{"dataset":"P","expected":["ID","VALUE"],"actual":["VALUE","ID"],"missing":[],"extra":[],"reordered":true}})
            ]
        );
        assert!(
            matches!(&error, yamaa_engine::producer_build::DecodeError::Metadata(held) if held.spec_paths == ["input.P.schema", "input.P.path"])
        );
    }
}
