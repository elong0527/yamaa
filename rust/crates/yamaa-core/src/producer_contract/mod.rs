//! Shared producer metadata only. These contracts grant no data or execution authority.
use crate::{
    schema::{Document, DocumentNode as N, SpecificationDocument},
    value::ColumnType,
};

pub(crate) mod diagnostics;

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
/// Each output schema has the fixed column boundary.
pub fn prepare(spec: &SpecificationDocument) -> Result<Contract, Error> {
    let d = spec.document();
    let declarations = sequence(d, field(d, d.root(), "columns")?)?;
    let output = field(d, d.root(), "output")?;
    let selected = sequence(d, field(d, output, "columns")?)?;
    crate::limits::Limits::columns(declarations.len()).map_err(Error::Limit)?;
    crate::limits::Limits::columns(selected.len()).map_err(Error::Limit)?;
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
