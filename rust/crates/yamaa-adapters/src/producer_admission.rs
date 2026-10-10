//! Admit supplied, already prepared producer documents without IO. Resolved
//! location facts are compared as identities; they are not resource capabilities.
use crate::specification_source::PreparedDocument;
use std::sync::Arc;
use yamaa_core::{
    producer_admission::{Candidate, Error as CoreError, Limits as CoreLimits},
    project_environment::ExecutionEnvironment,
    schema::DocumentNode as N,
};
use yamaa_engine::producer_admission::{self, CheckedProducerMetadata};

pub struct SuppliedProducer {
    pub dataset: String,
    pub schema_identity: String,
    pub source_identity: String,
    pub output_identity: String,
    pub document: Arc<PreparedDocument>,
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub captured_bytes: usize,
    pub schema_bytes: usize,
    pub schema_modules: usize,
    pub snapshots: usize,
    pub metadata: CoreLimits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            captured_bytes: 16_777_216,
            schema_bytes: 16_777_216,
            schema_modules: 128,
            snapshots: 1_024,
            metadata: Default::default(),
        }
    }
}
#[derive(Debug)]
pub enum Error {
    Limit(&'static str),
    Boundary,
    Admission(CoreError),
}
pub struct PreparedProducers {
    consumer: Arc<PreparedDocument>,
    producers: Vec<SuppliedProducer>,
    checked: CheckedProducerMetadata,
}
impl PreparedProducers {
    pub fn consumer(&self) -> &PreparedDocument {
        &self.consumer
    }
    pub fn producers(&self) -> &[SuppliedProducer] {
        &self.producers
    }
    pub fn checked(&self) -> &CheckedProducerMetadata {
        &self.checked
    }
}
/// Raw entry/parent bytes and independent layer origins remain held beside the
/// sealed core metadata. The caller supplies location facts obtained under the
/// existing approved path policy; this boundary neither resolves nor reads them.
pub(crate) fn candidates<'a>(
    consumer: &'a PreparedDocument,
    producers: &'a [SuppliedProducer],
    limits: Limits,
) -> Result<Vec<Candidate<'a>>, Error> {
    if producers.len() > limits.metadata.candidates {
        return Err(Error::Limit("producer_candidates"));
    }
    let mut bytes = 0usize;
    let mut nodes = 0usize;
    let mut identities = 0usize;
    let mut schema_bytes = 0usize;
    let mut snapshots = 0usize;
    for document in core::iter::once(consumer).chain(producers.iter().map(|p| p.document.as_ref()))
    {
        if document.schema().sources().len() > limits.schema_modules {
            return Err(Error::Limit("producer_schema_modules"));
        }
        for source in document.schema().sources() {
            identities = identities
                .checked_add(source.identity.len())
                .filter(|&n| n <= limits.metadata.text_bytes)
                .ok_or(Error::Limit("producer_metadata_text_bytes"))?;
            schema_bytes = schema_bytes
                .checked_add(source.bytes.len())
                .filter(|&n| n <= limits.schema_bytes)
                .ok_or(Error::Limit("producer_schema_bytes"))?;
        }
        nodes = nodes
            .checked_add(document.model().document().nodes().len())
            .filter(|&n| n <= limits.metadata.document_nodes)
            .ok_or(Error::Limit("producer_document_nodes"))?;
        identities = identities
            .checked_add(document.source().identity.len())
            .filter(|&n| n <= limits.metadata.text_bytes)
            .ok_or(Error::Limit("producer_metadata_text_bytes"))?;
        snapshots = snapshots
            .checked_add(document.parents().len().saturating_add(1))
            .filter(|&n| n <= limits.snapshots)
            .ok_or(Error::Limit("producer_snapshots"))?;
        for parent in document.parents() {
            identities = identities
                .checked_add(parent.source().identity.len())
                .filter(|&n| n <= limits.metadata.text_bytes)
                .ok_or(Error::Limit("producer_metadata_text_bytes"))?;
        }
        for length in core::iter::once(document.source().bytes.len())
            .chain(document.parents().iter().map(|p| p.source().bytes.len()))
        {
            bytes = bytes
                .checked_add(length)
                .filter(|&n| n <= limits.captured_bytes)
                .ok_or(Error::Limit("producer_captured_bytes"))?;
        }
    }
    for p in producers {
        for length in [
            p.dataset.len(),
            p.schema_identity.len(),
            p.source_identity.len(),
            p.output_identity.len(),
        ] {
            identities = identities
                .checked_add(length)
                .filter(|&n| n <= limits.metadata.text_bytes)
                .ok_or(Error::Limit("producer_metadata_text_bytes"))?;
        }
    }
    for p in producers {
        let expected = consumer.schema();
        let actual = p.document.schema();
        if expected.structure().root_class().name != actual.structure().root_class().name
            || expected.sources().len() != actual.sources().len()
            || expected.sources().iter().any(|source| {
                !actual
                    .sources()
                    .iter()
                    .any(|other| source.identity == other.identity && source.bytes == other.bytes)
            })
        {
            return Err(Error::Admission(CoreError::Invalid(vec![
                yamaa_core::producer_admission::schema_mismatch(&p.dataset),
            ])));
        }
    }
    // Shared identities mean the same retained bytes, including parent layers.
    // Compare held snapshots directly; no resource reopening or digest is used.
    for (index, p) in producers.iter().enumerate() {
        for snapshot in core::iter::once(p.document.source())
            .chain(p.document.parents().iter().map(|parent| parent.source()))
        {
            for prior in core::iter::once(consumer).chain(
                producers[..index]
                    .iter()
                    .map(|prior| prior.document.as_ref()),
            ) {
                if core::iter::once(prior.source())
                    .chain(prior.parents().iter().map(|parent| parent.source()))
                    .any(|other| {
                        snapshot.identity == other.identity && snapshot.bytes != other.bytes
                    })
                {
                    return Err(Error::Admission(CoreError::Invalid(vec![
                        yamaa_core::producer_admission::contradictory_snapshot(&p.dataset),
                    ])));
                }
            }
        }
    }
    let d = consumer.model().document();
    let input = d.field(d.root(), "input").ok_or(Error::Boundary)?;
    let candidates = producers
        .iter()
        .map(|p| {
            let schema = d
                .field(input, &p.dataset)
                .and_then(|declaration| d.field(declaration, "schema"));
            let Some(N::Text(schema_path)) = schema.map(|id| &d.nodes()[id]) else {
                return Err(Error::Admission(CoreError::Invalid(vec![
                    yamaa_core::producer_admission::undeclared_metadata(&p.dataset),
                ])));
            };
            Ok(Candidate {
                dataset: &p.dataset,
                schema_path,
                schema_origin: consumer
                    .written_input_origin(&p.dataset, "schema")
                    .ok_or(Error::Boundary)?,
                input_origin: consumer
                    .written_input_origin(&p.dataset, "path")
                    .ok_or(Error::Boundary)?,
                output_origin: p.document.written_output_origin().ok_or(Error::Boundary)?,
                schema_identity: &p.schema_identity,
                producer_identity: &p.document.source().identity,
                source_identity: &p.source_identity,
                output_identity: &p.output_identity,
                document: p.document.model(),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(candidates)
}

pub fn prepare(
    consumer: Arc<PreparedDocument>,
    producers: Vec<SuppliedProducer>,
    environment: Option<&ExecutionEnvironment>,
    limits: Limits,
) -> Result<PreparedProducers, Error> {
    let candidates = candidates(&consumer, &producers, limits)?;
    let checked = producer_admission::check(
        &consumer.source().identity,
        consumer.model(),
        &candidates,
        environment,
        limits.metadata,
    )
    .map_err(Error::Admission)?;
    Ok(PreparedProducers {
        consumer,
        producers,
        checked,
    })
}
