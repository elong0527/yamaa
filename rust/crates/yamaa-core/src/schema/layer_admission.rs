//! Admission of one inheritance contribution, before parent traversal or composition.

use super::{
    scalar_diagnostic_label, Document, DocumentNode as N, NormalizationBudget, NormalizationError,
    NormalizationResource, NormalizedDocument, SchemaContext as C, SchemaDiagnostic, SchemaField,
    SchemaOrigin, SchemaSource, SchemaStructure, ValidationError,
};
use alloc::{format, string::String, vec, vec::Vec};

#[derive(Clone, Copy)]
struct Collection {
    class: &'static str,
    identity: Option<&'static str>,
    fragment: bool,
}

fn collection(name: &str) -> Option<Collection> {
    let (class, identity, fragment) = match name {
        "input" => ("dataset_class", None, false),
        "columns" => ("column_class", Some("name"), true),
        "rows" => ("row_class", Some("id"), false),
        "intermediates" => ("intermediate_class", Some("id"), false),
        _ => return None,
    };
    Some(Collection {
        class,
        identity,
        fragment,
    })
}

struct Run<'a, 'b> {
    schema: &'a SchemaStructure,
    input: &'a Document,
    budget: &'b mut NormalizationBudget,
    findings: Vec<SchemaDiagnostic>,
    nodes: Vec<N>,
    origins: Vec<SchemaOrigin>,
}

impl Run<'_, '_> {
    fn depth(&mut self, depth: usize) -> Result<(), NormalizationError> {
        let limit = self.budget.limits.depth.min(128);
        if depth > limit {
            return Err(NormalizationError::Limit {
                resource: NormalizationResource::Depth,
                limit,
            });
        }
        self.budget.work(1)
    }

    fn field(
        &mut self,
        node: usize,
        name: &str,
    ) -> Result<Option<(usize, usize)>, NormalizationError> {
        let N::Mapping(entries) = &self.input.nodes()[node] else {
            return Err(ValidationError::InvalidDescriptor.into());
        };
        for &(key, value) in entries {
            self.budget.work(name.len().saturating_add(1))?;
            if matches!(&self.input.nodes()[key], N::Text(text) if text == name) {
                return Ok(Some((key, value)));
            }
        }
        Ok(None)
    }

    fn label(&mut self, node: usize) -> Result<String, NormalizationError> {
        let size = match &self.input.nodes()[node] {
            N::Text(text) | N::Integer(text) => text.len(),
            _ => 64,
        };
        self.budget.work(size.saturating_add(1))?;
        self.budget.validation.text(size)?;
        scalar_diagnostic_label(&self.input.nodes()[node])
            .ok_or_else(|| ValidationError::InvalidDescriptor.into())
    }

    fn join(&mut self, path: &str, name: &str) -> Result<String, NormalizationError> {
        let size = path.len().saturating_add(name.len()).saturating_add(1);
        self.budget.work(size)?;
        self.budget.validation.text(size)?;
        Ok(if path.is_empty() {
            name.into()
        } else {
            format!("{path}.{name}")
        })
    }

    fn indexed(&mut self, path: &str, label: &str) -> Result<String, NormalizationError> {
        let size = path.len().saturating_add(label.len()).saturating_add(2);
        self.budget.work(size)?;
        self.budget.validation.text(size)?;
        Ok(format!("{path}[{label}]"))
    }

    fn member_path(
        &mut self,
        path: &str,
        key: usize,
        label: &str,
    ) -> Result<String, NormalizationError> {
        if matches!(self.input.nodes()[key], N::Integer(_) | N::Boolean(_)) {
            self.indexed(path, label)
        } else {
            self.join(path, label)
        }
    }

    fn finding(
        &mut self,
        path: &str,
        condition: &'static str,
        requirement: &'static str,
        context: Vec<(&'static str, C)>,
    ) -> Result<(), NormalizationError> {
        self.findings.push(self.budget.validation.diagnostic(
            path,
            condition,
            Some(requirement),
            context,
        )?);
        Ok(())
    }

    fn bad_type(
        &mut self,
        node: usize,
        expected: &str,
        path: &str,
    ) -> Result<(), NormalizationError> {
        // Layer-shape errors retain Python type names, unlike R006 value diagnostics.
        let actual = match self.input.nodes()[node] {
            N::Null => "NoneType",
            N::Boolean(_) => "bool",
            N::Integer(_) => "int",
            N::Float(_) => "float",
            N::Text(_) => "str",
            N::Sequence(_) => "list",
            N::Mapping(_) => "dict",
        };
        self.finding(
            path,
            "invalid_field_type",
            "REQ-0658",
            vec![
                ("expected", C::Text(expected.into())),
                ("actual", C::Text(actual.into())),
            ],
        )
    }

    fn unknown_fields(
        &mut self,
        node: usize,
        fields: &[SchemaField],
        class: &str,
        path: &str,
    ) -> Result<(), NormalizationError> {
        let N::Mapping(entries) = &self.input.nodes()[node] else {
            unreachable!()
        };
        for &(key, _) in entries {
            let name = self.label(key)?;
            self.budget
                .work(fields.len().saturating_mul(name.len().saturating_add(1)))?;
            if !matches!(&self.input.nodes()[key], N::Text(text) if fields.iter().any(|f| f.name == *text))
            {
                // Root unknown fields use their plain spelling; nested integer/bool
                // names follow the host's bracketed path convention.
                let logical = if path.is_empty() {
                    self.join(path, &name)?
                } else {
                    self.member_path(path, key, &name)?
                };
                self.finding(
                    &logical,
                    "unknown_field",
                    "REQ-0658",
                    vec![("field", C::Text(name)), ("class", C::Text(class.into()))],
                )?;
            }
        }
        Ok(())
    }

    fn validate_value(
        &mut self,
        descriptor: usize,
        value: usize,
        path: &str,
        fragment: bool,
    ) -> Result<(), NormalizationError> {
        self.findings.extend(self.schema.validate_descriptor(
            descriptor,
            self.input,
            value,
            path,
            fragment,
            &mut self.budget.validation,
        )?);
        Ok(())
    }

    fn validate_type(
        &mut self,
        name: &str,
        value: usize,
        path: &str,
    ) -> Result<(), NormalizationError> {
        let types = self
            .schema
            .parse_query_types(&[name.into()], &mut self.budget.validation)?;
        self.findings.extend(self.schema.validate_types(
            &types,
            self.input,
            value,
            path,
            false,
            &mut self.budget.validation,
        )?);
        Ok(())
    }

    fn member(
        &mut self,
        node: usize,
        kind: Collection,
        path: &str,
    ) -> Result<(), NormalizationError> {
        self.depth(2)?;
        if !matches!(self.input.nodes()[node], N::Mapping(_)) {
            return self.bad_type(node, kind.class, path);
        }
        let fields = &self
            .schema
            .class_named(kind.class)
            .ok_or(ValidationError::InvalidDescriptor)?
            .fields;
        if let Some(identity) = kind.identity {
            if self.field(node, identity)?.is_none() {
                let logical = self.join(path, identity)?;
                self.finding(
                    &logical,
                    "missing_required_field",
                    "REQ-0658",
                    vec![
                        ("field", C::Text(identity.into())),
                        ("class", C::Text(kind.class.into())),
                    ],
                )?;
            }
        }
        self.unknown_fields(node, fields, kind.class, path)?;
        for field in fields {
            let Some((_, value)) = self.field(node, &field.name)? else {
                continue;
            };
            let logical = self.join(path, &field.name)?;
            if matches!(self.input.nodes()[value], N::Null) {
                if Some(field.name.as_str()) == kind.identity
                    || self.schema.descriptors()[field.descriptor]
                        .descriptor
                        .required()
                {
                    self.finding(
                        &logical,
                        "invalid_clear",
                        "REQ-0660",
                        vec![("field", C::Text(field.name.clone()))],
                    )?;
                }
            } else {
                self.validate_value(field.descriptor, value, &logical, kind.fragment)?;
            }
        }
        Ok(())
    }

    fn members(
        &mut self,
        node: usize,
        kind: Collection,
        path: &str,
    ) -> Result<(), NormalizationError> {
        self.depth(1)?;
        if let Some(identity) = kind.identity {
            let N::Sequence(items) = &self.input.nodes()[node] else {
                return self.bad_type(node, "list", path);
            };
            // Retained input occurrences avoid uncharged identity-string copies.
            let mut seen: Vec<usize> = Vec::new();
            self.budget.reserve(0, items.len())?;
            for (index, &member) in items.iter().enumerate() {
                let id = if matches!(self.input.nodes()[member], N::Mapping(_)) {
                    self.field(member, identity)?.map(|(_, value)| value)
                } else {
                    None
                };
                let id = id.filter(|&id| matches!(self.input.nodes()[id], N::Text(_)));
                let label = match id {
                    Some(id) => self.label(id)?,
                    None => format!("{index}"),
                };
                let logical = if id.is_some() {
                    self.join(path, &label)?
                } else {
                    self.indexed(path, &label)?
                };
                self.member(member, kind, &logical)?;
                if let Some(id) = id {
                    let mut duplicate = false;
                    for &prior in &seen {
                        self.budget.work(label.len().saturating_add(1))?;
                        if self.input.nodes()[prior] == self.input.nodes()[id] {
                            duplicate = true;
                            break;
                        }
                    }
                    if duplicate {
                        let logical = self.join(&logical, identity)?;
                        self.finding(
                            &logical,
                            "duplicate_identifier",
                            "REQ-0659",
                            vec![("identifier", C::Text(label))],
                        )?;
                    }
                    seen.push(id);
                }
            }
        } else {
            let N::Mapping(entries) = &self.input.nodes()[node] else {
                return self.bad_type(node, "dict", path);
            };
            for &(key, member) in entries {
                let label = self.label(key)?;
                let logical = self.member_path(path, key, &label)?;
                let key_path = self.join(path, &format!("key({label})"))?;
                self.validate_type("identifier", key, &key_path)?;
                if matches!(self.input.nodes()[member], N::Text(_)) {
                    self.validate_type("project_path", member, &logical)?;
                } else {
                    self.member(member, kind, &logical)?;
                }
            }
        }
        Ok(())
    }

    fn validate(&mut self) -> Result<(), NormalizationError> {
        self.depth(0)?;
        let root = self.input.root();
        if !matches!(&self.input.nodes()[root], N::Mapping(entries) if !entries.is_empty()) {
            return self.bad_type(root, "root_class", "$");
        }
        if self.field(root, "schema_version")?.is_none() {
            self.finding(
                "schema_version",
                "schema_version_mismatch",
                "REQ-0656",
                vec![
                    ("expected", C::Text(self.schema.version().into())),
                    ("actual", C::Null),
                ],
            )?;
        }
        let fields = &self.schema.root_class().fields;
        self.unknown_fields(root, fields, "root_class", "")?;
        for field in fields {
            let Some((_, value)) = self.field(root, &field.name)? else {
                continue;
            };
            if field.name == "parents" {
                self.validate_value(field.descriptor, value, &field.name, false)?;
            } else if matches!(self.input.nodes()[value], N::Null) {
                if self.schema.descriptors()[field.descriptor]
                    .descriptor
                    .required()
                {
                    self.finding(
                        &field.name,
                        "invalid_clear",
                        "REQ-0660",
                        vec![("field", C::Text(field.name.clone()))],
                    )?;
                }
            } else if let Some(kind) = collection(&field.name) {
                self.members(value, kind, &field.name)?;
            } else {
                self.validate_value(field.descriptor, value, &field.name, false)?;
            }
        }
        Ok(())
    }

    fn origin(&self, node: usize, generated: bool) -> SchemaOrigin {
        SchemaOrigin {
            source: SchemaSource::Input,
            node,
            generated,
        }
    }

    fn push(&mut self, node: N, origin: SchemaOrigin) -> usize {
        let id = self.nodes.len();
        self.nodes.push(node);
        self.origins.push(origin);
        id
    }

    fn scalar(&mut self, node: usize) -> Result<usize, NormalizationError> {
        let bytes = match &self.input.nodes()[node] {
            N::Text(text) | N::Integer(text) => text.len(),
            N::Sequence(_) | N::Mapping(_) => return Err(ValidationError::InvalidDescriptor.into()),
            _ => 0,
        };
        self.budget.work(bytes.saturating_add(1))?;
        self.budget.reserve(bytes, 0)?;
        Ok(self.push(self.input.nodes()[node].clone(), self.origin(node, false)))
    }

    fn import(&mut self, normalized: NormalizedDocument) -> Result<usize, NormalizationError> {
        let offset = self.nodes.len();
        for (node, origin) in normalized.document.nodes().iter().zip(normalized.origins) {
            let (bytes, edges) = match node {
                N::Text(text) | N::Integer(text) => (text.len(), 0),
                N::Sequence(items) => (0, items.len()),
                N::Mapping(entries) => (0, entries.len().saturating_mul(2)),
                _ => (0, 0),
            };
            self.budget
                .work(bytes.saturating_add(edges).saturating_add(1))?;
            self.budget.reserve(bytes, edges)?;
            let node = match node {
                N::Sequence(items) => N::Sequence(items.iter().map(|n| n + offset).collect()),
                N::Mapping(entries) => N::Mapping(
                    entries
                        .iter()
                        .map(|(k, v)| (k + offset, v + offset))
                        .collect(),
                ),
                node => node.clone(),
            };
            self.push(node, origin);
        }
        Ok(offset + normalized.document.root())
    }

    fn normalize_fields(
        &mut self,
        node: usize,
        fields: &[SchemaField],
        kind: Option<Collection>,
    ) -> Result<usize, NormalizationError> {
        self.depth(if kind.is_some() { 2 } else { 0 })?;
        self.budget.reserve(0, fields.len().saturating_mul(2))?;
        let mut output = Vec::with_capacity(fields.len());
        for field in fields {
            let Some((key, value)) = self.field(node, &field.name)? else {
                continue;
            };
            let key = self.scalar(key)?;
            let result = if matches!(self.input.nodes()[value], N::Null) {
                self.scalar(value)?
            } else if let Some(collection) = collection(&field.name).filter(|_| kind.is_none()) {
                self.normalize_members(value, collection)?
            } else {
                let normalized = self.schema.normalize_descriptor(
                    field.descriptor,
                    self.input,
                    value,
                    kind.is_some_and(|k| k.fragment),
                    self.budget,
                )?;
                self.import(normalized)?
            };
            output.push((key, result));
        }
        Ok(self.push(N::Mapping(output), self.origin(node, false)))
    }

    fn normalize_members(
        &mut self,
        node: usize,
        kind: Collection,
    ) -> Result<usize, NormalizationError> {
        self.depth(1)?;
        let fields = self
            .schema
            .class_named(kind.class)
            .map(|class| class.fields.as_slice());
        if kind.identity.is_some() {
            let N::Sequence(items) = &self.input.nodes()[node] else {
                unreachable!()
            };
            self.budget.reserve(0, items.len())?;
            let mut output = Vec::with_capacity(items.len());
            for &member in items {
                output.push(self.normalize_fields(
                    member,
                    fields.ok_or(ValidationError::InvalidDescriptor)?,
                    Some(kind),
                )?);
            }
            Ok(self.push(N::Sequence(output), self.origin(node, false)))
        } else {
            let N::Mapping(entries) = &self.input.nodes()[node] else {
                unreachable!()
            };
            self.budget.reserve(0, entries.len().saturating_mul(2))?;
            let mut output = Vec::with_capacity(entries.len());
            for &(key, member) in entries {
                let key = if matches!(self.input.nodes()[key], N::Text(_)) {
                    self.scalar(key)?
                } else {
                    let label = self.label(key)?;
                    self.budget.reserve(label.len(), 0)?;
                    self.push(N::Text(label), self.origin(key, true))
                };
                let value = if matches!(self.input.nodes()[member], N::Text(_)) {
                    self.budget.reserve(0, 2)?;
                    self.budget.reserve(4, 0)?;
                    let path = self.push(N::Text("path".into()), self.origin(member, true));
                    let value = self.scalar(member)?;
                    self.push(N::Mapping(vec![(path, value)]), self.origin(member, true))
                } else {
                    self.normalize_fields(
                        member,
                        fields.ok_or(ValidationError::InvalidDescriptor)?,
                        Some(kind),
                    )?
                };
                output.push((key, value));
            }
            Ok(self.push(N::Mapping(output), self.origin(node, false)))
        }
    }
}

impl SchemaStructure {
    /// Admit and normalize one schema-shaped inheritance contribution (REQ-0622-0626).
    /// All ordered field findings precede normalization; invalid input publishes no
    /// partial document. Parent traversal and role-specific version consistency remain
    /// caller responsibilities. Clear markers survive until composition.
    pub fn normalize_layer(
        &self,
        input: &Document,
        budget: &mut NormalizationBudget,
    ) -> Result<NormalizedDocument, NormalizationError> {
        let mut run = Run {
            schema: self,
            input,
            budget,
            findings: Vec::new(),
            nodes: Vec::new(),
            origins: Vec::new(),
        };
        run.validate()?;
        if !run.findings.is_empty() {
            return Err(NormalizationError::Invalid(run.findings));
        }
        let root = run.normalize_fields(input.root(), &self.root_class().fields, None)?;
        let document = Document::new(run.nodes, root, run.budget.limits.storage)
            .map_err(NormalizationError::Document)?;
        Ok(NormalizedDocument {
            document,
            origins: run.origins,
        })
    }
}
