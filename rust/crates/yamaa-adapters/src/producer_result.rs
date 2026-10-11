//! Public root results and explicit ordered publication from a sealed native owner.
//! Successful node reports still gate artifact consumption inside the build.
use crate::{
    issue_rows::Issue,
    producer_report::{self, NativeAttempt, ProjectionError},
    project_report::Classified,
    public_table::PublicTable,
    report_projection::Budget,
    specification_report::{Error as ReportError, Identity},
    table_transport::{self, TableTransportError},
};
use serde_json::Value;
use std::sync::Arc;
use yamaa_core::table::TableAccess;

pub struct RootResult {
    observations: Value,
    issues: Vec<Issue>,
    output: Option<(PublicTable, Vec<u8>)>,
    root: usize,
    logs: Arc<[crate::producer_report::NodeLogs]>,
    publication: Option<Arc<crate::producer_publication::Publication>>,
    publication_refusal: Option<&'static str>,
    base_issues: usize,
    latest: Option<Value>,
    save_attempt: Option<crate::producer_publication::Attempt>,
}
impl RootResult {
    pub fn observations(&self) -> &Value {
        self.latest.as_ref().unwrap_or(&self.observations)
    }
    pub fn issues(&self) -> &[Issue] {
        &self.issues
    }
    pub fn output(&self) -> Option<&[u8]> {
        self.output.as_ref().map(|(_, bytes)| bytes.as_slice())
    }
    pub fn output_table(&self) -> Option<&PublicTable> {
        self.output.as_ref().map(|(table, _)| table)
    }
    fn held_log(&self, warning: bool) -> Option<&crate::verification_logs::HeldLog> {
        if warning && self.output.is_none() {
            return None;
        }
        let logs = &self.logs.iter().find(|logs| logs.node == self.root)?.logs;
        (if warning {
            &logs.warning
        } else {
            &logs.verification
        })
        .as_ref()
    }
    pub fn verification_log(&self) -> Option<&[u8]> {
        self.held_log(false).map(|log| log.ipc())
    }
    pub fn warning_log(&self) -> Option<&[u8]> {
        self.held_log(true).map(|log| log.ipc())
    }
    pub fn log_table(&self, warning: bool) -> Option<&PublicTable> {
        self.held_log(warning).map(|log| log.table())
    }
    /// Retain the original operational failure and actual entered prefix beside
    /// the unchanged build owner. Reading evidence grants no publication port.
    pub fn publication_attempt(&self) -> Option<&crate::producer_publication::Attempt> {
        self.save_attempt.as_ref()
    }
    pub fn save(&mut self) -> Result<bool, crate::file_application::FailedBuild> {
        self.save_with_limits(Default::default(), 16_777_216)
    }
    pub fn save_with_limits(
        &mut self,
        limits: yamaa_engine::producer_publication::Limits,
        maximum: usize,
    ) -> Result<bool, crate::file_application::FailedBuild> {
        if self.output.is_none()
            || (self.publication.is_none() && self.publication_refusal.is_none())
        {
            return Err(crate::file_application::FailedBuild);
        }
        self.issues.truncate(self.base_issues);
        self.latest = None;
        self.save_attempt = None;
        let Some(publication) = self.publication.as_ref() else {
            self.issues.extend(crate::file_application::rejected(
                "output",
                self.publication_refusal.unwrap(),
            ));
            return Ok(false);
        };
        // Complete report ownership and prefix projection admission precedes
        // every publisher. Save never classifies a host error or rerenders Build.
        let mut budget = Budget::new(maximum);
        let admitted = (|| {
            budget.json(&self.observations)?;
            budget.entries(publication.artifacts().len().saturating_mul(64))?;
            for artifact in publication.artifacts() {
                for _ in 0..4 {
                    budget.text(artifact.path())?;
                }
            }
            Ok::<_, ReportError>(())
        })();
        if admitted.is_err() {
            self.issues.extend(crate::file_application::rejected(
                "output",
                "publication_report_limit",
            ));
            return Ok(false);
        }
        let mut value = self.observations.clone();
        let mut attempt = crate::producer_publication::Attempt::default();
        publication.save_into(limits, &mut attempt);
        use yamaa_engine::producer_publication::Failure;
        let (saved, code, node) = if let Some(failure) = publication.admission_failure() {
            use crate::producer_publication::AdmissionFailure;
            match failure {
                AdmissionFailure::Incomplete => (false, "publication_incomplete", None),
                AdmissionFailure::Limit(code) => (false, *code, None),
                AdmissionFailure::Resource { node, error } => {
                    self.issues
                        .extend(crate::file_application::resource_failure("output", error));
                    (false, "target_admission", Some(*node))
                }
            }
        } else if attempt.unwind.is_some() {
            (false, "publication_unwind", None)
        } else {
            match &attempt.entered.outcome {
                Ok(()) => (true, "", None),
                Err(Failure::Incomplete) => (false, "publication_incomplete", None),
                Err(Failure::Limit(code)) => (false, *code, None),
                Err(Failure::Preflight(error)) => {
                    self.issues
                        .extend(crate::file_application::publication_failure(&error.error));
                    (false, "target_preflight", Some(error.node))
                }
                Err(Failure::Publish { node, error }) => {
                    self.issues
                        .extend(crate::file_application::publication_failure(&error.error));
                    if let Some(artifact) =
                        publication.artifacts().iter().find(|a| a.node() == *node)
                    {
                        self.issues.push(
                            Issue::from_core(yamaa_core::application_issue::publication_failed(
                                artifact.path(),
                            ))
                            .expect("publication issue"),
                        );
                    }
                    (false, "publication_failed", Some(*node))
                }
            }
        };
        if !saved && self.issues.len() == self.base_issues {
            self.issues
                .extend(crate::file_application::rejected("output", code));
        }
        value["publication"] = serde_json::json!({"status":if saved {"success"} else {"failure"},
            "entered":attempt.entered.entered.iter().map(|item|serde_json::json!({"node":item.node,"published":item.published})).collect::<Vec<_>>(),
            "error":if saved {Value::Null} else {serde_json::json!({"stage":"publication","code":code,"node":node})}});
        self.latest = Some(value);
        self.save_attempt = Some(attempt);
        Ok(saved)
    }
}

/// Refusal preserves the original whole attempt and, when already completed,
/// its bounded report. There is no partial successful public root result.
pub enum Error<'a, E> {
    Report(ProjectionError<'a, E>),
    Output {
        attempt: &'a NativeAttempt<E>,
        observations: Value,
        error: TableTransportError,
    },
}
impl<E> std::fmt::Debug for Error<'_, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Report(error) => f.debug_tuple("Report").field(error).finish(),
            Self::Output { error, .. } => f.debug_tuple("Output").field(error).finish(),
        }
    }
}

/// Both host facts and root projection originate in this exact whole owner.
/// A failed graph may retain earlier producer artifacts as evidence, but has
/// no accepted public output. No metadata, study or host port is entered here.
pub fn prepare_classified<'a, E>(
    attempt: &'a NativeAttempt<E>,
    id: Identity<'_>,
    maximum: usize,
    facts: &[Classified<'a, E>],
) -> Result<RootResult, Error<'a, E>> {
    let observations = producer_report::attempt_report_classified(attempt, id, maximum, facts)
        .map_err(Error::Report)?;
    let mut budget = Budget::new(maximum);
    let diagnostics =
        observations["diagnostics"]
            .as_array()
            .ok_or(Error::Report(ProjectionError::Report(
                ReportError::InvalidObservation,
            )))?;
    let admit = (|| {
        budget.entries(diagnostics.len())?;
        for diagnostic in diagnostics {
            // JSON clone, five-field owned rows and escaped context text.
            for _ in 0..3 {
                budget.json(diagnostic)?;
            }
        }
        Ok::<_, ReportError>(())
    })();
    admit.map_err(|error| Error::Report(ProjectionError::Report(error)))?;
    let mut issues = diagnostics
        .iter()
        .cloned()
        .map(Issue::from_diagnostic)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::Report(ProjectionError::Report(ReportError::InvalidObservation)))?;
    if !attempt.accepted() && issues.is_empty() {
        issues = crate::file_application::rejected("build", "producer_build");
    }
    if !crate::issue_rows::within_limit(
        &issues,
        maximum.min(crate::specification_check::MAX_ISSUE_BYTES),
    ) {
        return Err(Error::Report(ProjectionError::Report(
            ReportError::OutputLimit,
        )));
    }
    let output = if attempt.accepted() {
        match root_output(attempt, &mut budget) {
            Ok(output) => Some(output),
            Err(error) => {
                return Err(Error::Output {
                    attempt,
                    observations,
                    error,
                })
            }
        }
    } else {
        None
    };
    Ok(RootResult {
        observations,
        base_issues: issues.len(),
        issues,
        output,
        root: attempt.provenance.metadata().root(),
        logs: Arc::from([]),
        publication: None,
        publication_refusal: None,
        latest: None,
        save_attempt: None,
    })
}

/// Only a privately constructed actual build owner can grant Save. Report and
/// public root projection must both finish before that authority is attached.
pub fn prepare_owned_classified<'a, E>(
    attempt: &'a crate::file_producer_build::OwnedAttempt<E>,
    id: Identity<'_>,
    maximum: usize,
    facts: &[Classified<'a, E>],
) -> Result<RootResult, Error<'a, E>> {
    let mut result = prepare_classified(attempt, id, maximum, facts)?;
    result.logs = Arc::clone(attempt.logs());
    if result.output.is_some() {
        result.publication = attempt.publication().cloned();
        result.publication_refusal = attempt.publication_refusal();
    }
    Ok(result)
}

fn root_output<E>(
    attempt: &NativeAttempt<E>,
    budget: &mut Budget,
) -> Result<(PublicTable, Vec<u8>), TableTransportError> {
    let invalid = TableTransportError::InvalidTable;
    let metadata = attempt.provenance.metadata();
    // Root identity and compiler are borrowed from the sealed complete owner.
    budget
        .entries(metadata.nodes().len())
        .map_err(|_| TableTransportError::OutputLimit)?;
    budget
        .work(
            metadata
                .nodes()
                .len()
                .checked_mul(3)
                .ok_or(TableTransportError::OutputLimit)?,
        )
        .map_err(|_| TableTransportError::OutputLimit)?;
    let root = metadata.root();
    let view = attempt.provenance.report_views().nth(root).ok_or(invalid)?;
    let mut matches = attempt.graph.nodes.iter().filter(|node| node.node == root);
    let entered = matches.next().ok_or(invalid)?;
    if matches.next().is_some() {
        return Err(invalid);
    }
    let execution = entered
        .dataset
        .result
        .as_ref()
        .map_err(|_| invalid)?
        .result
        .as_ref()
        .map_err(|_| invalid)?;
    let artifact = entered
        .output
        .as_ref()
        .map_err(|_| invalid)?
        .as_ref()
        .and_then(|output| output.artifact())
        .ok_or(invalid)?;
    let table = &execution.dataset;
    let columns = table.schema().columns();
    let projection = artifact.projection();
    budget
        .entries(projection.len())
        .map_err(|_| TableTransportError::OutputLimit)?;
    budget
        .work(
            projection
                .len()
                .checked_mul(columns.len())
                .ok_or(TableTransportError::OutputLimit)?,
        )
        .map_err(|_| TableTransportError::OutputLimit)?;
    if projection.len() != view.compiled().projection().len()
        || projection
            .iter()
            .zip(view.compiled().projection())
            .any(|(&index, name)| columns.get(index).is_none_or(|column| &column.name != name))
    {
        return Err(invalid);
    }
    // The qualified encoder sanitizes the complete table before projection.
    // Admit full shape, names, cells and work for Arrow, public columns and IPC
    // before any of those owners allocate. Serialized CSV precision is irrelevant.
    for _ in 0..3 {
        budget
            .shape(table.row_count(), columns.len())
            .map_err(|_| TableTransportError::OutputLimit)?;
        for (index, column) in columns.iter().enumerate() {
            budget
                .text(&column.name)
                .map_err(|_| TableTransportError::OutputLimit)?;
            for row in 0..table.row_count() {
                let cell = table.cell(row, index).map_err(|_| invalid)?;
                budget
                    .cell(&cell)
                    .map_err(|_| TableTransportError::OutputLimit)?;
            }
        }
    }
    let bytes = table_transport::encode_projected_dataset(table, projection)?;
    let table = PublicTable::decode(&bytes)?;
    let bytes = table.ipc()?;
    Ok((table, bytes))
}
