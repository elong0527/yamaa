//! Repository fixture binding facts from immutable schemas, without data ports.
use alloc::{collections::BTreeSet, string::String, vec::Vec};
use yamaa_core::{
    diagnostic::Diagnostic,
    specification::{BindError, PreparedSpecification, SourceInterpretation, SourceProfile},
    table::{Column, SchemaError, TableAccess, TableSchema},
    value::ColumnType,
};

pub use yamaa_core::specification::SourceProfile as Profile;
pub use yamaa_core::value::ColumnType as Kind;

pub struct Origin<'a> {
    pub dataset: &'a str,
    pub declaring_source: &'a str,
    pub written: &'a str,
}
pub struct InputMetadata {
    pub owner: String,
    pub dataset: String,
    pub declaring_source: String,
    pub written: String,
    pub profile: Profile,
}

/// Capture descriptors retain their original declaring identities. This grants
/// no bytes, interpretation, inspection result, execution or publication.
pub fn inputs(
    owner: &str,
    prepared: &PreparedSpecification,
    origins: &[Origin<'_>],
    limits: Limits,
) -> Result<Vec<InputMetadata>, Error> {
    let mut budget = Budget::new(origins.len(), limits)?;
    if origins.len() != prepared.sources().len() {
        return Err(Error::SourceIdentity);
    }
    for (origin, source) in origins.iter().zip(prepared.sources()) {
        if origin.dataset != source.name
            || origin.declaring_source.is_empty()
            || origin.written.is_empty()
        {
            return Err(Error::SourceIdentity);
        }
        for text in [
            owner,
            origin.dataset,
            origin.declaring_source,
            origin.written,
        ] {
            budget.text(text.len())?;
        }
    }
    Ok(origins
        .iter()
        .zip(prepared.sources())
        .map(|(origin, source)| InputMetadata {
            owner: owner.into(),
            dataset: origin.dataset.into(),
            declaring_source: origin.declaring_source.into(),
            written: origin.written.into(),
            profile: source.profile,
        })
        .collect())
}

#[derive(Clone, Copy)]
pub struct Limits {
    pub sources: usize,
    pub columns: usize,
    pub text: usize,
    pub findings: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            sources: 1024,
            columns: 4096,
            text: 1_048_576,
            findings: 4096,
        }
    }
}

/// Count complete carriers before a host representation makes owned copies.
pub struct Budget {
    limits: Limits,
    columns: usize,
    text: usize,
}
impl Budget {
    pub fn new(sources: usize, limits: Limits) -> Result<Self, Error> {
        if sources == 0 || sources > limits.sources {
            return Err(Error::Limit);
        }
        Ok(Self {
            limits,
            columns: 0,
            text: 0,
        })
    }
    pub fn columns(&mut self, amount: usize) -> Result<(), Error> {
        self.columns = self
            .columns
            .checked_add(amount)
            .filter(|n| *n <= self.limits.columns)
            .ok_or(Error::Limit)?;
        Ok(())
    }
    pub fn text(&mut self, amount: usize) -> Result<(), Error> {
        self.text = self
            .text
            .checked_add(amount)
            .filter(|n| *n <= self.limits.text)
            .ok_or(Error::Limit)?;
        Ok(())
    }
}

pub struct SourceSchema {
    pub dataset: String,
    pub schema: TableSchema,
}
impl SourceSchema {
    pub fn new(dataset: String, columns: Vec<Column>) -> Result<Self, Error> {
        if dataset.is_empty()
            || dataset.contains('\0')
            || columns.is_empty()
            || columns.iter().any(|column| column.name.contains('\0'))
        {
            return Err(Error::InvalidCarrier);
        }
        Ok(Self {
            dataset,
            schema: TableSchema::new(columns).map_err(Error::Schema)?,
        })
    }
}

#[derive(Debug)]
pub enum Error {
    Limit,
    InvalidCarrier,
    Schema(SchemaError),
    SourceIdentity,
    Binding(BindError),
    Projection,
    Graph(yamaa_core::producer_graph::MetadataBindingError),
}

/// Only the held compiled owner and complete metadata are accepted. A successful
/// result proves names/types in this scope, never source values or execution.
pub fn check(
    prepared: &PreparedSpecification,
    sources: &[SourceSchema],
    limits: Limits,
) -> Result<Vec<Diagnostic>, Error> {
    let mut budget = Budget::new(sources.len(), limits)?;
    if prepared.sources().len() != sources.len() {
        return Err(Error::SourceIdentity);
    }
    let mut names = BTreeSet::new();
    for (source, declared) in sources.iter().zip(prepared.sources()) {
        if source.dataset != declared.name || !names.insert(&source.dataset) {
            return Err(Error::SourceIdentity);
        }
        budget.text(source.dataset.len())?;
        budget.columns(source.schema.columns().len())?;
        for column in source.schema.columns() {
            budget.text(column.name.len())?;
        }
    }
    let logical = match normalize(
        prepared.sources(),
        |index| prepared.source_interpretation(index),
        sources,
        limits,
    )? {
        Ok(schemas) => schemas,
        Err(diagnostics) => return Ok(diagnostics),
    };
    let mut diagnostics = Vec::new();
    let schemas = logical.iter().collect::<Vec<_>>();
    match prepared.bind_sources(&schemas) {
        Ok(_) => Ok(Vec::new()),
        Err(BindError::Invalid(findings)) => {
            if findings.len() > limits.findings {
                return Err(Error::Limit);
            }
            for finding in findings {
                let projected = finding
                    .diagnostics(prepared.source())
                    .ok_or(Error::Projection)?;
                if projected.len() > limits.findings.saturating_sub(diagnostics.len()) {
                    return Err(Error::Limit);
                }
                diagnostics.extend(projected);
            }
            Ok(diagnostics)
        }
        Err(error) => Err(Error::Binding(error)),
    }
}

fn normalize(
    declarations: &[yamaa_core::specification::SourceDeclaration],
    interpretation: impl Fn(usize) -> Option<SourceInterpretation>,
    sources: &[SourceSchema],
    limits: Limits,
) -> Result<Result<Vec<TableSchema>, Vec<Diagnostic>>, Error> {
    let mut logical = Vec::new();
    let mut diagnostics = Vec::new();
    for (index, (source, declared)) in sources.iter().zip(declarations).enumerate() {
        let schema = if interpretation(index) == Some(SourceInterpretation::Odm) {
            let fields = source
                .schema
                .columns()
                .iter()
                .map(|column| yamaa_core::odm_source::StoredField {
                    name: &column.name,
                    kind: Some(column.kind),
                })
                .collect::<Vec<_>>();
            match yamaa_core::odm_source::bind_fields(&fields).map_err(|_| Error::Limit)? {
                Ok(_) => TableSchema::new(
                    yamaa_core::odm_source::FIELDS
                        .iter()
                        .map(|name| Column {
                            name: (*name).into(),
                            kind: ColumnType::Str,
                        })
                        .collect(),
                )
                .map_err(Error::Schema)?,
                Err(findings) => {
                    for finding in findings {
                        diagnostics.push(
                            finding
                                .finding(&source.dataset)
                                .diagnostic()
                                .map_err(|_| Error::Projection)?,
                        );
                    }
                    continue;
                }
            }
        } else if declared.profile == SourceProfile::Csv {
            if source
                .schema
                .columns()
                .iter()
                .any(|column| column.kind != ColumnType::Str)
            {
                return Err(Error::InvalidCarrier);
            }
            let names = source
                .schema
                .columns()
                .iter()
                .map(|column| column.name.clone())
                .collect();
            match yamaa_core::typed_csv::PreparedTypes::new(&declared.types, limits.columns)
                .map_err(|_| Error::InvalidCarrier)?
                .convert(yamaa_core::csv_source::CsvSource {
                    names,
                    records: Vec::new(),
                }) {
                Ok(rows) => rows.schema().clone(),
                Err(finding) => {
                    diagnostics.push(
                        finding
                            .finding(&source.dataset)
                            .ok_or(Error::Projection)?
                            .diagnostic()
                            .map_err(|_| Error::Projection)?,
                    );
                    continue;
                }
            }
        } else {
            source.schema.clone()
        };
        let schema = match crate::source_ingestion::ordinal(
            declared,
            &schema,
            0,
            crate::source_ingestion::Limits {
                columns: limits.columns,
                cells: 0,
            },
        ) {
            Ok(Some(ordinal)) => ordinal.schema().clone(),
            Ok(None) => schema,
            Err(finding) => {
                diagnostics.push(
                    finding
                        .finding(
                            &source.dataset,
                            declared.ordinal.as_deref().ok_or(Error::Projection)?,
                        )
                        .ok_or(Error::Projection)?
                        .diagnostic()
                        .map_err(|_| Error::Projection)?,
                );
                continue;
            }
        };
        logical.push(schema);
    }
    if diagnostics.len() > limits.findings {
        return Err(Error::Limit);
    }
    if !diagnostics.is_empty() {
        return Ok(Err(diagnostics));
    }
    Ok(Ok(logical))
}

/// External metadata carries both the held graph node and the authored dataset.
/// Producer fields are supplied only by the admitted native output contract.
pub struct GraphSourceSchema {
    pub owner: String,
    pub source: SourceSchema,
}

pub fn graph_check(
    graph: &yamaa_core::producer_graph::PreparedGraph,
    external: &[GraphSourceSchema],
    limits: Limits,
) -> Result<Vec<(usize, Diagnostic)>, Error> {
    let mut budget = Budget::new(external.len(), limits)?;
    let expected = graph
        .nodes()
        .iter()
        .map(|node| node.sources().len().saturating_sub(node.producers().len()))
        .sum::<usize>();
    if external.len() != expected {
        return Err(Error::SourceIdentity);
    }
    // Admit the complete external carrier before native producer-schema clones.
    for source in external {
        budget.text(source.owner.len())?;
        budget.text(source.source.dataset.len())?;
        budget.columns(source.source.schema.columns().len())?;
        for column in source.source.schema.columns() {
            budget.text(column.name.len())?;
        }
    }
    let mut next = 0;
    for node in graph.nodes() {
        for declaration in node.sources() {
            if let Some(producer) = node
                .producers()
                .iter()
                .find(|p| p.dataset() == declaration.name)
            {
                let fields = producer.contract().fields();
                budget.columns(fields.len())?;
                for field in fields {
                    budget.text(field.name.len())?;
                }
            } else {
                let actual = external.get(next).ok_or(Error::SourceIdentity)?;
                next += 1;
                if actual.owner != node.identity() || actual.source.dataset != declaration.name {
                    return Err(Error::SourceIdentity);
                }
            }
        }
    }
    let mut next = 0;
    let mut schemas = Vec::new();
    let mut diagnostics = Vec::new();
    for (index, node) in graph.nodes().iter().enumerate() {
        let mut complete = Vec::new();
        for (source_index, declaration) in node.sources().iter().enumerate() {
            if let Some(producer) = node
                .producers()
                .iter()
                .find(|p| p.dataset() == declaration.name)
            {
                let fields = producer.contract().fields();
                complete.push(
                    TableSchema::new(
                        fields
                            .iter()
                            .map(|field| Column {
                                name: field.name.clone(),
                                kind: field.kind,
                            })
                            .collect(),
                    )
                    .map_err(Error::Schema)?,
                );
            } else {
                let actual = external.get(next).ok_or(Error::SourceIdentity)?;
                next += 1;
                if actual.owner != node.identity() || actual.source.dataset != declaration.name {
                    return Err(Error::SourceIdentity);
                }
                match normalize(
                    core::slice::from_ref(declaration),
                    |_| node.source_interpretation(source_index),
                    core::slice::from_ref(&actual.source),
                    limits,
                )? {
                    Ok(mut schemas) => complete.push(schemas.remove(0)),
                    Err(findings) => {
                        diagnostics.extend(findings.into_iter().map(|finding| (index, finding)))
                    }
                }
            }
        }
        schemas.push(complete);
    }
    if diagnostics.len() > limits.findings {
        return Err(Error::Limit);
    }
    if !diagnostics.is_empty() {
        return Ok(diagnostics);
    }
    graph
        .check_source_schemas(&schemas, limits.findings)
        .map_err(Error::Graph)
}

pub const MAX_CAPTURE_BYTES: usize = 8_388_608;

/// A closed slot and caller ceiling are admitted before a resource port enters.
pub fn admit_capture(index: usize, slots: usize, maximum: usize) -> Result<(), Error> {
    if maximum == 0 || maximum > MAX_CAPTURE_BYTES || slots > Limits::default().sources {
        return Err(Error::Limit);
    }
    if index >= slots {
        return Err(Error::SourceIdentity);
    }
    Ok(())
}

pub struct GraphOrigin<'a> {
    pub owner: &'a str,
    pub origin: Origin<'a>,
}

pub fn graph_inputs(
    graph: &yamaa_core::producer_graph::PreparedGraph,
    origins: &[GraphOrigin<'_>],
    limits: Limits,
) -> Result<Vec<InputMetadata>, Error> {
    let mut budget = Budget::new(origins.len(), limits)?;
    let mut next = 0;
    for node in graph.nodes() {
        for source in node.sources() {
            if node
                .producers()
                .iter()
                .any(|producer| producer.dataset() == source.name)
            {
                continue;
            }
            let supplied = origins.get(next).ok_or(Error::SourceIdentity)?;
            next += 1;
            if supplied.owner != node.identity()
                || supplied.origin.dataset != source.name
                || supplied.origin.declaring_source.is_empty()
                || supplied.origin.written.is_empty()
            {
                return Err(Error::SourceIdentity);
            }
            for text in [
                supplied.owner,
                supplied.origin.dataset,
                supplied.origin.declaring_source,
                supplied.origin.written,
            ] {
                budget.text(text.len())?;
            }
        }
    }
    if next != origins.len() {
        return Err(Error::SourceIdentity);
    }
    let mut descriptors = Vec::new();
    let mut next = 0;
    for node in graph.nodes() {
        for source in node.sources() {
            if node
                .producers()
                .iter()
                .any(|producer| producer.dataset() == source.name)
            {
                continue;
            }
            let supplied = &origins[next];
            next += 1;
            descriptors.push(InputMetadata {
                owner: supplied.owner.into(),
                dataset: supplied.origin.dataset.into(),
                declaring_source: supplied.origin.declaring_source.into(),
                written: supplied.origin.written.into(),
                profile: source.profile,
            });
        }
    }
    Ok(descriptors)
}
