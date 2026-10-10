//! Sealed compiler admission for producer metadata. This is not an executable
//! workflow: the private compiled representation cannot become a dataset plan.
use crate::{
    diagnostic::{ConditionCode as C, Diagnostic},
    producer_contract::{self, diagnostics::text, Contract},
    project_environment::ExecutionEnvironment,
    schema::{Document, DocumentNode as N, SpecificationDocument},
    specification::{
        CompilationLimits, PrepareError, PreparedSpecification, SourceDeclaration,
        UnsupportedFeature,
    },
    table::TableSchema,
};
use alloc::{format, string::String, vec, vec::Vec};

/// A spelling and the retained specification layer that authored it. These are
/// provenance facts, not a filesystem lookup or a resource capability.
#[derive(Clone, Copy, Debug)]
pub struct WrittenPath<'a> {
    pub declaring_source: &'a str,
    pub written: &'a str,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    declaring_source: String,
    written: String,
}
impl Origin {
    pub fn declaring_source(&self) -> &str {
        &self.declaring_source
    }
    pub fn written(&self) -> &str {
        &self.written
    }
}
/// Prepared documents and already resolved location facts supplied by the
/// surrounding preparation boundary. Equality grants no resource authority.
pub struct Candidate<'a> {
    pub dataset: &'a str,
    pub schema_path: &'a str,
    pub schema_origin: WrittenPath<'a>,
    pub input_origin: WrittenPath<'a>,
    pub output_origin: WrittenPath<'a>,
    pub schema_identity: &'a str,
    pub producer_identity: &'a str,
    pub source_identity: &'a str,
    pub output_identity: &'a str,
    pub document: &'a SpecificationDocument,
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub candidates: usize,
    pub document_nodes: usize,
    pub text_bytes: usize,
    pub contract: producer_contract::Limits,
    pub compilation: CompilationLimits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            candidates: 16,
            document_nodes: 200_000,
            text_bytes: 1_048_576,
            contract: Default::default(),
            compilation: Default::default(),
        }
    }
}
#[derive(Debug)]
pub enum Error {
    Limit(&'static str),
    Invalid(Vec<Diagnostic>),
    Compilation(PrepareError),
    Boundary,
}
#[derive(Debug)]
pub struct Declaration {
    dataset: String,
    schema_path: String,
    schema_origin: Origin,
    input_origin: Origin,
    output_origin: Origin,
    producer_identity: String,
    artifact_identity: String,
    contract: Contract,
}
impl Declaration {
    pub fn dataset(&self) -> &str {
        &self.dataset
    }
    pub fn schema_path(&self) -> &str {
        &self.schema_path
    }
    pub fn schema_origin(&self) -> &Origin {
        &self.schema_origin
    }
    pub fn input_origin(&self) -> &Origin {
        &self.input_origin
    }
    pub fn output_origin(&self) -> &Origin {
        &self.output_origin
    }
    pub fn producer_identity(&self) -> &str {
        &self.producer_identity
    }
    pub fn artifact_identity(&self) -> &str {
        &self.artifact_identity
    }
    pub fn contract(&self) -> &Contract {
        &self.contract
    }
}
/// Input order is authored order. A producer has a distinct identity and contract
/// and cannot be projected as an ordinary external study-file declaration.
#[derive(Debug)]
pub enum Input<'a> {
    External(&'a SourceDeclaration),
    Producer(&'a Declaration),
}
#[derive(Debug)]
pub struct Prepared {
    compiled: PreparedSpecification,
    producers: Vec<Declaration>,
}
impl Prepared {
    pub fn producers(&self) -> &[Declaration] {
        &self.producers
    }
    pub fn inputs(&self) -> impl Iterator<Item = Input<'_>> {
        self.compiled.sources().iter().map(|s| {
            self.producers
                .iter()
                .find(|p| p.dataset == s.name)
                .map_or(Input::External(s), Input::Producer)
        })
    }
    pub fn output(&self) -> &TableSchema {
        self.compiled.output()
    }
    pub fn projection(&self) -> &[String] {
        self.compiled.projection()
    }
    pub fn called_functions(&self) -> &[usize] {
        self.compiled.called_functions()
    }
    /// No bind/build or compiled-plan accessor is exposed. The engine must refuse
    /// an execution capability until the complete workflow qualifies under #1741.
    pub fn execution_refusal(&self) -> PrepareError {
        PrepareError::Unsupported(
            self.producers
                .iter()
                .map(|p| UnsupportedFeature {
                    operation: "producer_workflow".into(),
                    path: format!("input.{}.schema", p.dataset),
                })
                .collect(),
        )
    }
}
fn scalar(d: &Document, id: usize) -> Result<&str, Error> {
    match &d.nodes()[id] {
        N::Text(s) => Ok(s),
        _ => Err(Error::Boundary),
    }
}
fn invalid(dataset: &str, reason: &str) -> Diagnostic {
    Diagnostic {
        code: C::ProducerInvalidContract,
        spec_paths: vec![format!("input.{dataset}.schema")],
        context: [
            ("dataset".into(), text(dataset)),
            ("reason".into(), text(reason)),
        ]
        .into_iter()
        .collect(),
        source_span: None,
        operand_route: None,
    }
}
/// A supplied producer was prepared against a different captured root closure.
/// The adapter charges dataset text before projecting this portable cause.
pub fn schema_mismatch(dataset: &str) -> Diagnostic {
    invalid(dataset, "schema_mismatch")
}
fn charge(
    total: &mut usize,
    count: usize,
    ceiling: usize,
    name: &'static str,
) -> Result<(), Error> {
    *total = total
        .checked_add(count)
        .filter(|&n| n <= ceiling)
        .ok_or(Error::Limit(name))?;
    Ok(())
}
/// Compile consumer declarations against bounded producer output metadata only.
/// Unsupported consumer vocabulary, conflicting authorities and incomplete
/// metadata fail here, without a source, activation or runtime callback port.
pub fn prepare(
    consumer_identity: &str,
    consumer: &SpecificationDocument,
    candidates: &[Candidate<'_>],
    environment: Option<&ExecutionEnvironment>,
    limits: Limits,
) -> Result<Prepared, Error> {
    if candidates.len() > limits.candidates {
        return Err(Error::Limit("producer_candidates"));
    }
    let d = consumer.document();
    let Some(N::Mapping(inputs)) = d.field(d.root(), "input").map(|id| &d.nodes()[id]) else {
        return Err(Error::Boundary);
    };
    if inputs.len() > limits.compilation.inputs {
        return Err(Error::Limit("inputs"));
    }
    let mut nodes = 0usize;
    let mut bytes = 0usize;
    charge(
        &mut bytes,
        consumer_identity.len(),
        limits.text_bytes,
        "producer_metadata_text_bytes",
    )?;
    for document in core::iter::once(consumer).chain(candidates.iter().map(|c| c.document)) {
        charge(
            &mut nodes,
            document.document().nodes().len(),
            limits.document_nodes,
            "producer_document_nodes",
        )?;
        for node in document.document().nodes() {
            if let N::Text(s) | N::Integer(s) = node {
                charge(
                    &mut bytes,
                    s.len(),
                    limits.text_bytes,
                    "producer_metadata_text_bytes",
                )?;
            }
        }
    }
    for c in candidates {
        for s in [
            c.dataset,
            c.schema_path,
            c.schema_origin.declaring_source,
            c.schema_origin.written,
            c.input_origin.declaring_source,
            c.input_origin.written,
            c.output_origin.declaring_source,
            c.output_origin.written,
            c.schema_identity,
            c.producer_identity,
            c.source_identity,
            c.output_identity,
        ] {
            charge(
                &mut bytes,
                s.len(),
                limits.text_bytes,
                "producer_metadata_text_bytes",
            )?;
        }
    }
    // Preflight all declared type authorities before validating any producer.
    let mut findings = Vec::new();
    for &(name, id) in inputs {
        let dataset = scalar(d, name)?;
        if d.field(id, "schema")
            .is_none_or(|id| matches!(d.nodes()[id], N::Null))
        {
            continue;
        }
        if let Some(types) = d.field(id, "types") {
            let declared = match &d.nodes()[types] {
                N::Mapping(fields) => fields.as_slice(),
                N::Null => &[],
                _ => return Err(Error::Boundary),
            };
            if declared.len() > limits.compilation.source_fields {
                return Err(Error::Limit("source_fields"));
            }
            let mut push = |field: &str, value: &str| {
                findings.push(Diagnostic {
                    code: C::ProducerRedundantSourceType,
                    spec_paths: vec![if field.is_empty() {
                        format!("input.{dataset}.types")
                    } else {
                        format!("input.{dataset}.types.{field}")
                    }],
                    context: [
                        ("dataset".into(), text(dataset)),
                        ("field".into(), text(field)),
                        ("type".into(), text(value)),
                    ]
                    .into_iter()
                    .collect(),
                    source_span: None,
                    operand_route: None,
                });
            };
            if declared.is_empty() {
                push("", "");
            }
            for &(field, value) in declared {
                push(scalar(d, field)?, scalar(d, value)?);
            }
        }
    }
    if !findings.is_empty() {
        return Err(Error::Invalid(findings));
    }
    let mut producers = Vec::new();
    for &(name, id) in inputs {
        let dataset = scalar(d, name)?;
        let Some(schema) = d
            .field(id, "schema")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        else {
            continue;
        };
        let schema_path = scalar(d, schema)?;
        let matches = candidates
            .iter()
            .filter(|c| c.dataset == dataset)
            .collect::<Vec<_>>();
        let [candidate] = matches.as_slice() else {
            findings.push(invalid(
                dataset,
                if matches.is_empty() {
                    "missing_metadata"
                } else {
                    "duplicate_metadata"
                },
            ));
            continue;
        };
        let c = *candidate;
        if c.schema_path != schema_path
            || c.schema_identity != c.producer_identity
            || [
                consumer_identity,
                c.schema_identity,
                c.source_identity,
                c.output_identity,
                c.schema_origin.declaring_source,
                c.schema_origin.written,
                c.input_origin.declaring_source,
                c.input_origin.written,
                c.output_origin.declaring_source,
                c.output_origin.written,
            ]
            .contains(&"")
        {
            findings.push(invalid(dataset, "contradictory_metadata"));
            continue;
        }
        if c.producer_identity == consumer_identity {
            findings.push(invalid(dataset, "producer_workflow_cycle"));
            continue;
        }
        let contract = match producer_contract::prepare(c.document, limits.contract) {
            Ok(contract) => contract,
            Err(producer_contract::Error::Invalid(causes)) => {
                findings.extend(causes.iter().map(|cause| cause.diagnostic(dataset)));
                continue;
            }
            Err(producer_contract::Error::Limit(name)) => return Err(Error::Limit(name)),
            Err(producer_contract::Error::Boundary) => return Err(Error::Boundary),
        };
        if contract.fields().len() > limits.compilation.source_fields {
            return Err(Error::Limit("source_fields"));
        }
        if c.source_identity != c.output_identity {
            findings.push(Diagnostic {
                code: C::ProducerOutputPathMismatch,
                spec_paths: vec![
                    format!("input.{dataset}.path"),
                    format!("input.{dataset}.schema"),
                ],
                context: [
                    ("dataset".into(), text(dataset)),
                    ("source_path".into(), text(c.input_origin.written)),
                    ("output_path".into(), text(c.output_origin.written)),
                ]
                .into_iter()
                .collect(),
                source_span: None,
                operand_route: None,
            });
            continue;
        }
        producers.push(Declaration {
            dataset: dataset.into(),
            schema_path: schema_path.into(),
            schema_origin: Origin {
                declaring_source: c.schema_origin.declaring_source.into(),
                written: c.schema_origin.written.into(),
            },
            input_origin: Origin {
                declaring_source: c.input_origin.declaring_source.into(),
                written: c.input_origin.written.into(),
            },
            output_origin: Origin {
                declaring_source: c.output_origin.declaring_source.into(),
                written: c.output_origin.written.into(),
            },
            producer_identity: c.producer_identity.into(),
            artifact_identity: c.output_identity.into(),
            contract,
        });
    }
    for c in candidates {
        if !inputs.iter().any(|&(name, id)| {
            scalar(d, name).is_ok_and(|name| name == c.dataset)
                && d.field(id, "schema")
                    .is_some_and(|id| !matches!(d.nodes()[id], N::Null))
        }) {
            findings.push(invalid(c.dataset, "undeclared_metadata"));
        }
    }
    if !findings.is_empty() {
        return Err(Error::Invalid(findings));
    }
    if producers.is_empty() {
        return Err(Error::Boundary);
    }
    let compiled = PreparedSpecification::prepare_producer_metadata(
        consumer,
        limits.compilation,
        environment,
        &producers,
    )
    .map_err(Error::Compilation)?;
    Ok(Prepared {
        compiled,
        producers,
    })
}
