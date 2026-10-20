//! Producing-specification links: type metadata only. A link names the
//! specification whose output contract types a stored artifact; it grants no
//! data, execution or publication authority, and the producer is never run.
use crate::{
    diagnostic::{ConditionCode as C, Diagnostic},
    schema::{Document, DocumentNode as N, SpecificationDocument},
    value::ColumnType,
};
use std::vec;

pub(crate) mod diagnostics;
use diagnostics::text;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cause {
    Empty,
    Duplicate {
        position: usize,
        field: String,
    },
    Declaration {
        position: usize,
        field: String,
        count: usize,
    },
    Label {
        field: String,
    },
}
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("producer contract limit: {0:?}")]
    Limit(crate::limits::LimitExceeded),
    #[error("producer contract invalid: {0:?}")]
    Invalid(Vec<Cause>),
    #[error("producer contract boundary")]
    Boundary,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub kind: ColumnType,
    pub label: String,
}
/// Only admission can construct this ordered contract from an admitted specification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contract {
    fields: Vec<Field>,
}
impl Contract {
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }
    /// INPUT-11 compares actual stored headers in their original order. It never
    /// sorts, infers a type, or reads a value to establish this contract.
    pub fn matches_header(&self, actual: &[&str]) -> bool {
        actual.len() == self.fields.len()
            && actual.iter().zip(&self.fields).all(|(a, b)| *a == b.name)
    }
    pub fn matches_typed_fields(&self, actual: &[(&str, ColumnType)]) -> bool {
        actual.len() == self.fields.len()
            && actual
                .iter()
                .zip(&self.fields)
                .all(|((name, kind), field)| *name == field.name && *kind == field.kind)
    }
}
fn field(d: &Document, node: usize, name: &str) -> Result<usize, Error> {
    d.field(node, name).ok_or(Error::Boundary)
}
fn node_text(d: &Document, node: usize) -> Result<&str, Error> {
    match &d.nodes()[node] {
        N::Text(s) => Ok(s),
        _ => Err(Error::Boundary),
    }
}
fn sequence(d: &Document, node: usize) -> Result<&[usize], Error> {
    match &d.nodes()[node] {
        N::Sequence(s) => Ok(s),
        _ => Err(Error::Boundary),
    }
}
fn kind(d: &Document, node: usize) -> Result<ColumnType, Error> {
    match node_text(d, node)? {
        "str" => Ok(ColumnType::Str),
        "int" => Ok(ColumnType::Int),
        "float" => Ok(ColumnType::Float),
        "date" => Ok(ColumnType::Date),
        "datetime" => Ok(ColumnType::DateTime),
        _ => Err(Error::Boundary),
    }
}
/// Read the producing specification's selected output fields with their declared
/// types and labels. Each output schema has the fixed column boundary.
pub fn prepare(spec: &SpecificationDocument) -> Result<Contract, Error> {
    let d = spec.document();
    let declarations = sequence(d, field(d, d.root(), "columns")?)?;
    let output = field(d, d.root(), "output")?;
    let selected = sequence(d, field(d, output, "columns")?)?;
    crate::limits::Limits::columns(declarations.len()).map_err(Error::Limit)?;
    crate::limits::Limits::columns(selected.len()).map_err(Error::Limit)?;
    let mut fields = Vec::new();
    let mut findings = Vec::new();
    if selected.is_empty() {
        findings.push(Cause::Empty);
    }
    for (position, &id) in selected.iter().enumerate() {
        let name = node_text(d, id)?;
        if selected[..position]
            .iter()
            .any(|&prior| node_text(d, prior).is_ok_and(|prior| prior == name))
        {
            findings.push(Cause::Duplicate {
                position,
                field: name.into(),
            });
            continue;
        }
        let mut matching = None;
        let mut count = 0usize;
        for &column in declarations {
            if node_text(d, field(d, column, "name")?)? == name {
                matching = Some(column);
                count += 1;
            }
        }
        if count != 1 {
            findings.push(Cause::Declaration {
                position,
                field: name.into(),
                count,
            });
            continue;
        }
        let column = matching.ok_or(Error::Boundary)?;
        let label = d
            .field(column, "label")
            .and_then(|id| node_text(d, id).ok());
        let Some(label) = label.filter(|value| !value.trim().is_empty()) else {
            findings.push(Cause::Label { field: name.into() });
            continue;
        };
        fields.push(Field {
            name: name.into(),
            kind: kind(d, field(d, column, "type")?)?,
            label: label.into(),
        });
    }
    if !findings.is_empty() {
        return Err(Error::Invalid(findings));
    }
    Ok(Contract { fields })
}

/// One `schema` link whose producing specification the surrounding boundary
/// has read and validated as a specification document.
#[derive(Clone, Copy, Debug)]
pub struct Link<'a> {
    /// The consuming input dataset that declares the link.
    pub dataset: &'a str,
    /// Canonical identity of the producing specification.
    pub identity: &'a str,
    pub document: &'a SpecificationDocument,
}
/// Every resolved link of one consuming specification, beside the consumer's
/// own canonical identity. The default supplies no link metadata.
#[derive(Clone, Copy, Debug, Default)]
pub struct Links<'a> {
    pub consumer: &'a str,
    pub links: &'a [Link<'a>],
}

#[derive(Debug)]
pub(crate) enum AdmissionError {
    Limit(crate::limits::LimitExceeded),
    Invalid(Vec<Diagnostic>),
    Boundary,
}

fn scalar(d: &Document, id: usize) -> Result<&str, AdmissionError> {
    match &d.nodes()[id] {
        N::Text(s) => Ok(s),
        _ => Err(AdmissionError::Boundary),
    }
}
/// A nullable schema field does not declare a link.
fn is_linked(document: &Document, declaration: usize) -> bool {
    document
        .field(declaration, "schema")
        .is_some_and(|id| !matches!(document.nodes()[id], N::Null))
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

/// Admit every linked input of `consumer` before compilation. `types` must be
/// absent beside `schema`, a link cannot name the consuming specification, and
/// the producing specification must declare a valid output contract. Returns
/// each linked dataset's contract in authored input order.
pub(crate) fn admit(
    consumer: &SpecificationDocument,
    links: Links<'_>,
) -> Result<Vec<(String, Contract)>, AdmissionError> {
    crate::limits::Limits::inputs(links.links.len()).map_err(AdmissionError::Limit)?;
    let d = consumer.document();
    let Some(N::Mapping(inputs)) = d.field(d.root(), "input").map(|id| &d.nodes()[id]) else {
        return Err(AdmissionError::Boundary);
    };
    crate::limits::Limits::inputs(inputs.len()).map_err(AdmissionError::Limit)?;
    // Preflight every declared type authority before reading any contract.
    let mut findings = Vec::new();
    for &(name, id) in inputs {
        let dataset = scalar(d, name)?;
        if !is_linked(d, id) {
            continue;
        }
        if let Some(types) = d.field(id, "types") {
            let declared = match &d.nodes()[types] {
                N::Mapping(fields) => fields.as_slice(),
                N::Null => &[],
                _ => return Err(AdmissionError::Boundary),
            };
            crate::limits::Limits::columns(declared.len()).map_err(AdmissionError::Limit)?;
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
        return Err(AdmissionError::Invalid(findings));
    }
    let mut contracts: Vec<(String, Contract)> = Vec::new();
    let mut identities: Vec<&str> = Vec::new();
    for &(name, id) in inputs {
        let dataset = scalar(d, name)?;
        if !is_linked(d, id) {
            continue;
        }
        let matches = links
            .links
            .iter()
            .filter(|link| link.dataset == dataset)
            .collect::<Vec<_>>();
        let [link] = matches.as_slice() else {
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
        if links.consumer.is_empty() || link.identity.is_empty() {
            return Err(AdmissionError::Boundary);
        }
        if link.identity == links.consumer {
            findings.push(invalid(dataset, "self_reference"));
            continue;
        }
        let contract = match prepare(link.document) {
            Ok(contract) => contract,
            Err(Error::Invalid(causes)) => {
                findings.extend(causes.iter().map(|cause| cause.diagnostic(dataset)));
                continue;
            }
            Err(Error::Limit(limit)) => return Err(AdmissionError::Limit(limit)),
            Err(Error::Boundary) => return Err(AdmissionError::Boundary),
        };
        crate::limits::Limits::columns(contract.fields().len()).map_err(AdmissionError::Limit)?;
        // One producing specification has one output contract, whichever
        // inputs and spellings name it.
        if identities
            .iter()
            .zip(&contracts)
            .any(|(identity, (_, prior))| *identity == link.identity && *prior != contract)
        {
            findings.push(invalid(dataset, "contradictory_metadata"));
            continue;
        }
        identities.push(link.identity);
        contracts.push((dataset.to_owned(), contract));
    }
    for link in links.links {
        if !inputs.iter().any(|&(name, id)| {
            scalar(d, name).is_ok_and(|name| name == link.dataset) && is_linked(d, id)
        }) {
            findings.push(invalid(link.dataset, "undeclared_metadata"));
        }
    }
    if !findings.is_empty() {
        return Err(AdmissionError::Invalid(findings));
    }
    Ok(contracts)
}
