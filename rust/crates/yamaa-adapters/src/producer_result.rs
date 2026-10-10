//! Read-only public root results from the sealed complete native attempt.
//! Successful node reports still gate artifact consumption inside the build.
//! This boundary has no publication port and cannot produce a Save capability.
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
use yamaa_core::table::TableAccess;

pub struct RootResult {
    observations: Value,
    issues: Vec<Issue>,
    output: Option<(PublicTable, Vec<u8>)>,
}
impl RootResult {
    pub fn observations(&self) -> &Value {
        &self.observations
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
        issues,
        output,
    })
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
