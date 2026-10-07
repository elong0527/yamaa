//! Final normalized model admission. This does not validate authored schema,
//! metadata relationships, expression payload semantics or execution support.
mod rules;

use super::{
    Document, DocumentNode as N, SchemaContext, SchemaDiagnostic, ValidationBudget, ValidationError,
};
use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

#[derive(Clone, Copy)]
enum Kind {
    Text,
    Integer,
    Boolean,
    Json,
    Expression,
    Ordinal,
    NonemptyText,
    NonemptyStrings,
    Enum(&'static [&'static str]),
    List(&'static Kind),
    Dictionary(&'static Kind),
    Optional(&'static Kind),
    Object(Model),
    Union(&'static [(&'static str, Kind)]),
}
#[derive(Clone, Copy)]
struct Field {
    name: &'static str,
    kind: Kind,
    required: bool,
}
#[derive(Clone, Copy)]
struct Model {
    name: &'static str,
    fields: &'static [Field],
}

/// An owned normalized document that passed the closed structural model contract.
/// Construction is private; a successful admission says nothing about executable scope.
#[derive(Clone, Debug, PartialEq)]
pub struct SpecificationDocument {
    document: Document,
}

impl SpecificationDocument {
    /// Consume the document only after every structural finding has been collected.
    /// Related schema passes can share this budget; failed union branches retain charges.
    pub fn admit(
        document: Document,
        budget: &mut ValidationBudget,
    ) -> Result<Result<Self, Vec<SchemaDiagnostic>>, ValidationError> {
        let diagnostics = Run {
            document: &document,
            budget,
        }
        .check(document.root(), Kind::Object(rules::SPECIFICATION), "", 0)?;
        Ok(if diagnostics.is_empty() {
            Ok(Self { document })
        } else {
            Err(diagnostics)
        })
    }
    /// Stable occurrence IDs retain declaration order and externally held provenance.
    pub fn document(&self) -> &Document {
        &self.document
    }
    /// Explicit base wins; multiple unselected sources have no implicit driver.
    pub fn default_driver(&self) -> Option<&str> {
        let root = self.document.root();
        if let Some(id) = self.document.field(root, "base") {
            if let N::Text(value) = &self.document.nodes()[id] {
                return Some(value);
            }
        }
        let input = self.document.field(root, "input")?;
        let N::Mapping(entries) = &self.document.nodes()[input] else {
            return None;
        };
        if entries.len() != 1 {
            return None;
        }
        match &self.document.nodes()[entries[0].0] {
            N::Text(value) => Some(value),
            _ => None,
        }
    }
}

struct Run<'a> {
    document: &'a Document,
    budget: &'a mut ValidationBudget,
}
impl Run<'_> {
    fn path(&mut self, parent: &str, child: &str) -> Result<String, ValidationError> {
        self.budget
            .text(parent.len().saturating_add(child.len()).saturating_add(1))?;
        Ok(if parent.is_empty() {
            child.into()
        } else {
            format!("{parent}.{child}")
        })
    }
    fn finding(
        &mut self,
        path: &str,
        reason: impl Into<String>,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        Ok(vec![self.budget.diagnostic(
            path,
            "model_contract_mismatch",
            None,
            vec![("reason", SchemaContext::Text(reason.into()))],
        )?])
    }
    fn key(&mut self, id: usize) -> Result<String, ValidationError> {
        // Model error locations encode bool keys as integer locations, unlike
        // schema diagnostics. The key remains a bool and is still rejected.
        let node = &self.document.nodes()[id];
        self.budget.work(node.length().unwrap_or(1))?;
        match node {
            N::Boolean(true) => Ok("1".into()),
            N::Boolean(false) => Ok("0".into()),
            _ => super::scalar_diagnostic_label(node).ok_or(ValidationError::InvalidDescriptor),
        }
    }
    fn check(
        &mut self,
        id: usize,
        kind: Kind,
        path: &str,
        depth: usize,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        self.budget.depth(depth)?;
        self.budget.work(1)?;
        let node = &self.document.nodes()[id];
        match kind {
            Kind::Optional(inner) if !matches!(node, N::Null) => {
                self.check(id, *inner, path, depth + 1)
            }
            Kind::Optional(_) => Ok(Vec::new()),
            Kind::Object(model) => self.object(id, model, path, depth + 1),
            Kind::Union(branches) => {
                let mut failures = Vec::new();
                for (name, branch) in branches {
                    let prefix = self.path(path, name)?;
                    let result = self.check(id, *branch, &prefix, depth + 1)?;
                    if result.is_empty() {
                        return Ok(Vec::new());
                    }
                    failures.extend(result);
                }
                Ok(failures)
            }
            Kind::List(_) | Kind::NonemptyStrings => {
                let N::Sequence(items) = node else {
                    return self.finding(path, "Input should be a valid list");
                };
                let item = if let Kind::List(item) = kind {
                    *item
                } else {
                    Kind::Text
                };
                let mut result = Vec::new();
                for (index, id) in items.iter().enumerate() {
                    let p = self.path(path, &index.to_string())?;
                    result.extend(self.check(*id, item, &p, depth + 1)?);
                }
                if result.is_empty() && matches!(kind, Kind::NonemptyStrings) && items.is_empty() {
                    return self.finding(
                        path,
                        "List should have at least 1 item after validation, not 0",
                    );
                }
                Ok(result)
            }
            Kind::Dictionary(item) => self.dictionary(id, *item, path, depth + 1),
            Kind::Json => match node {
                N::Sequence(_) => {
                    let p = self.path(path, "list")?;
                    self.check(id, Kind::List(&Kind::Json), &p, depth + 1)
                }
                N::Mapping(_) => {
                    let p = self.path(path, "dict")?;
                    self.dictionary(id, Kind::Json, &p, depth + 1)
                }
                _ => Ok(Vec::new()),
            },
            Kind::Expression => {
                let result = self.dictionary(id, Kind::Json, path, depth + 1)?;
                if result.is_empty() && matches!(node, N::Mapping(items) if items.len() != 1) {
                    self.finding(
                        path,
                        "Value error, an expression must contain exactly one operation",
                    )
                } else {
                    Ok(result)
                }
            }
            Kind::Enum(values) => {
                self.budget.work(
                    values
                        .len()
                        .saturating_mul(node.length().unwrap_or(1).saturating_add(1)),
                )?;
                if matches!(node, N::Text(text) if values.contains(&text.as_str())) {
                    return Ok(Vec::new());
                }
                let mut expected = values.iter().map(|s| format!("'{s}'")).collect::<Vec<_>>();
                let last = expected.pop().unwrap_or_default();
                let expected = if expected.is_empty() {
                    last
                } else {
                    format!("{} or {last}", expected.join(", "))
                };
                self.finding(path, format!("Input should be {expected}"))
            }
            Kind::Text | Kind::NonemptyText | Kind::Ordinal => {
                let N::Text(text) = node else {
                    return self.finding(path, "Input should be a valid string");
                };
                self.budget.work(text.len())?;
                if matches!(kind, Kind::NonemptyText) && text.is_empty() {
                    return self.finding(path, "String should have at least 1 character");
                }
                if matches!(kind, Kind::Ordinal)
                    && !(text
                        .as_bytes()
                        .first()
                        .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
                        && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
                {
                    return self.finding(
                        path,
                        "String should match pattern '^[A-Za-z_][A-Za-z0-9_]*$'",
                    );
                }
                Ok(Vec::new())
            }
            Kind::Integer if !matches!(node, N::Integer(_)) => {
                self.finding(path, "Input should be a valid integer")
            }
            Kind::Boolean if !matches!(node, N::Boolean(_)) => {
                self.finding(path, "Input should be a valid boolean")
            }
            Kind::Integer | Kind::Boolean => Ok(Vec::new()),
        }
    }
    fn dictionary(
        &mut self,
        id: usize,
        item: Kind,
        path: &str,
        depth: usize,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        let N::Mapping(entries) = &self.document.nodes()[id] else {
            return self.finding(path, "Input should be a valid dictionary");
        };
        let mut result = Vec::new();
        for &(key, value) in entries {
            let name = self.key(key)?;
            let p = self.path(path, &name)?;
            if !matches!(self.document.nodes()[key], N::Text(_)) {
                let p = self.path(&p, "[key]")?;
                result.extend(self.finding(&p, "Input should be a valid string")?);
            }
            result.extend(self.check(value, item, &p, depth + 1)?);
        }
        Ok(result)
    }
    fn object(
        &mut self,
        id: usize,
        model: Model,
        path: &str,
        depth: usize,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        let N::Mapping(entries) = &self.document.nodes()[id] else {
            return self.finding(
                path,
                format!(
                    "Input should be a valid dictionary or instance of {}",
                    model.name
                ),
            );
        };
        let mut result = Vec::new();
        // Match the public model's field order, followed by authored extra-field order.
        for field in model.fields {
            self.budget.work(
                entries
                    .len()
                    .saturating_mul(field.name.len().saturating_add(1)),
            )?;
            let value = self.document.field(id, field.name);
            if value.is_some() || field.required {
                let p = self.path(path, field.name)?;
                result.extend(if let Some(value) = value {
                    self.check(value, field.kind, &p, depth + 1)?
                } else {
                    self.finding(&p, "Field required")?
                });
            }
        }
        for &(key, _) in entries {
            let name = self.key(key)?;
            self.budget.work(
                model
                    .fields
                    .len()
                    .saturating_mul(name.len().saturating_add(1)),
            )?;
            if !matches!(self.document.nodes()[key], N::Text(_)) {
                let p = self.path(path, &name)?;
                result.extend(self.finding(&p, "Keys should be strings")?);
            } else if !model.fields.iter().any(|field| field.name == name) {
                let p = self.path(path, &name)?;
                result.extend(self.finding(&p, "Extra inputs are not permitted")?);
            }
        }
        if result.is_empty() {
            if model.name == "IntermediateUnique"
                && self
                    .document
                    .field(id, "id")
                    .is_some_and(|id| matches!(self.document.nodes()[id], N::Null))
            {
                return self.finding(
                    path,
                    "Value error, an explicit verification id must be nonempty text",
                );
            }
            if model.name == "IntermediateVerification" && self.document.field(id, "unique").is_some_and(|id| matches!(&self.document.nodes()[id], N::Sequence(items) if items.is_empty())) {
                return self.finding(path, "Value error, a uniqueness check requires at least one column");
            }
        }
        Ok(result)
    }
}
