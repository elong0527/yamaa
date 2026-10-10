//! Shared producer metadata only. These contracts grant no data or execution authority.
use crate::{
    schema::{Document, DocumentNode as N, SpecificationDocument},
    value::ColumnType,
};
use alloc::{string::String, vec::Vec};

#[path = "producer_contract_diagnostics.rs"]
pub(crate) mod diagnostics;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub declarations: usize,
    pub fields: usize,
    pub text_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            declarations: 64,
            fields: 64,
            text_bytes: 262_144,
        }
    }
}
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
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Limit(&'static str),
    Invalid(Vec<Cause>),
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
    path: String,
    fields: Vec<Field>,
}
impl Contract {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }
    /// REQ-0535 compares actual stored headers in their original order. It never
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
fn text(d: &Document, node: usize) -> Result<&str, Error> {
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
    match text(d, node)? {
        "str" => Ok(ColumnType::Str),
        "int" => Ok(ColumnType::Int),
        "float" => Ok(ColumnType::Float),
        "date" => Ok(ColumnType::Date),
        "datetime" => Ok(ColumnType::DateTime),
        _ => Err(Error::Boundary),
    }
}
/// Validate producer output fields and labels before any activation or study port.
/// All owned strings and findings follow aggregate quota admission.
pub fn prepare(spec: &SpecificationDocument, limits: Limits) -> Result<Contract, Error> {
    let d = spec.document();
    let declarations = sequence(d, field(d, d.root(), "columns")?)?;
    let output = field(d, d.root(), "output")?;
    let selected = sequence(d, field(d, output, "columns")?)?;
    if declarations.len() > limits.declarations {
        return Err(Error::Limit("producer_declarations"));
    }
    if selected.len() > limits.fields {
        return Err(Error::Limit("producer_fields"));
    }
    let mut bytes = 0usize;
    for node in d.nodes() {
        if let N::Text(s) | N::Integer(s) = node {
            bytes = bytes
                .checked_add(s.len())
                .filter(|&n| n <= limits.text_bytes)
                .ok_or(Error::Limit("producer_text_bytes"))?;
        }
    }
    let path = text(d, field(d, output, "path")?)?;
    let mut fields = Vec::new();
    let mut findings = Vec::new();
    if selected.is_empty() {
        findings.push(Cause::Empty);
    }
    for (position, &id) in selected.iter().enumerate() {
        let name = text(d, id)?;
        if selected[..position]
            .iter()
            .any(|&prior| text(d, prior).is_ok_and(|prior| prior == name))
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
            if text(d, field(d, column, "name")?)? == name {
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
        let label = d.field(column, "label").and_then(|id| text(d, id).ok());
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
    Ok(Contract {
        path: path.into(),
        fields,
    })
}
