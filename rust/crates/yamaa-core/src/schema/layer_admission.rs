//! Admission of one inheritance contribution, before parent traversal or composition.

use super::{
    scalar_diagnostic_label, validation::NullPolicy, Document, DocumentKind, DocumentNode as N,
    NormalizationError, NormalizationResource, NormalizedDocument, SchemaClass, SchemaContext as C,
    SchemaDiagnostic, SchemaField, SchemaOrigin, SchemaSource, SchemaStructure, ValidationError,
};
use crate::diagnostic::ConditionCode;
use std::{collections::BTreeMap, vec};

/// A keyed root collection. Every member is a fragment whose requiredness is
/// deferred at every depth, and whose nulls are clear markers (LAYER-4).
#[derive(Clone, Copy)]
struct Collection<'s> {
    /// The member class, when the collection's type names one.
    class: Option<&'s SchemaClass>,
    name: &'s str,
    identity: Option<&'static str>,
    /// `input`: a bare path is the dataset shorthand.
    path_shorthand: bool,
}
impl<'s> Collection<'s> {
    fn fields(&self) -> Result<&'s [SchemaField], NormalizationError> {
        self.class
            .map(|class| class.fields.as_slice())
            .ok_or_else(|| ValidationError::InvalidDescriptor.into())
    }
}

fn collection<'s>(schema: &'s SchemaStructure, field: &SchemaField) -> Option<Collection<'s>> {
    let (fallback, identity, path_shorthand) = match field.name.as_str() {
        "input" => ("dataset_class", None, true),
        "windows" => ("window_spec", None, false),
        "columns" => ("column_class", Some("name"), false),
        "rows" => ("row_class", Some("id"), false),
        "intermediates" => ("intermediate_class", Some("id"), false),
        _ => return None,
    };
    let class = schema.member_class(field.descriptor);
    Some(Collection {
        class,
        name: class.map_or(fallback, |class| class.name.as_str()),
        identity,
        path_shorthand,
    })
}

struct Run<'a> {
    schema: &'a SchemaStructure,
    input: &'a Document,
    findings: Vec<SchemaDiagnostic>,
    nodes: Vec<N>,
    origins: Vec<SchemaOrigin>,
}

impl Run<'_> {
    fn depth(&mut self, depth: usize) -> Result<(), NormalizationError> {
        let limit = crate::limits::Limits::SEMANTIC_DEPTH;
        if depth > limit {
            return Err(NormalizationError::Limit {
                resource: NormalizationResource::Depth,
                limit,
            });
        }
        Ok(())
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
            if matches!(&self.input.nodes()[key], N::Text(text) if text == name) {
                return Ok(Some((key, value)));
            }
        }
        Ok(None)
    }

    fn label(&mut self, node: usize) -> Result<String, NormalizationError> {
        scalar_diagnostic_label(&self.input.nodes()[node])
            .ok_or_else(|| ValidationError::InvalidDescriptor.into())
    }

    fn join(&mut self, path: &str, name: &str) -> Result<String, NormalizationError> {
        Ok(if path.is_empty() {
            name.into()
        } else {
            format!("{path}.{name}")
        })
    }

    fn indexed(&mut self, path: &str, label: &str) -> Result<String, NormalizationError> {
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
        code: ConditionCode,
        context: Vec<(&'static str, C)>,
    ) -> Result<(), NormalizationError> {
        self.findings
            .push(super::validation::diagnostic(path, code, context));
        Ok(())
    }

    fn bad_type(&mut self, node: usize, expected: C, path: &str) -> Result<(), NormalizationError> {
        let actual = self.input.nodes()[node].kind();
        self.finding(
            path,
            ConditionCode::SchemaLayerInvalidFieldType,
            vec![("expected", expected), ("actual", C::LayerKind(actual))],
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
                    ConditionCode::SchemaLayerUnknownField,
                    super::validation::unknown_field_context(&name, class),
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
        // A member value keeps its nulls as clear markers; a whole root value
        // replaces what it inherits, so its nested nulls are simply absent.
        let nulls = if fragment {
            NullPolicy::Clear
        } else {
            NullPolicy::Absent
        };
        self.findings.extend(
            self.schema
                .validate_descriptor_with(descriptor, self.input, value, path, fragment, nulls)?,
        );
        Ok(())
    }

    fn validate_type(
        &mut self,
        name: &str,
        value: usize,
        path: &str,
    ) -> Result<(), NormalizationError> {
        let types = self.schema.parse_query_types(&[name.into()])?;
        self.findings.extend(
            self.schema
                .validate_types(&types, self.input, value, path, false)?,
        );
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
            return self.bad_type(node, C::Text(kind.name.into()), path);
        }
        let fields = kind.fields()?;
        if let Some(identity) = kind.identity {
            if self.field(node, identity)?.is_none() {
                let logical = self.join(path, identity)?;
                self.finding(
                    &logical,
                    ConditionCode::SchemaLayerMissingRequiredField,
                    vec![
                        ("field", C::Text(identity.into())),
                        ("class", C::Text(kind.name.into())),
                    ],
                )?;
            }
        }
        self.unknown_fields(node, fields, kind.name, path)?;
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
                        ConditionCode::SchemaLayerInvalidClear,
                        vec![("field", C::Text(field.name.clone()))],
                    )?;
                }
            } else {
                self.validate_value(field.descriptor, value, &logical, true)?;
            }
        }
        Ok(())
    }

    /// SCHEMA-15: a dataset written as a mapping that holds only `path` restates
    /// the bare path it is the shorthand of. A clear marker beside it adds to it.
    fn restated_path(&mut self, member: usize, path: &str) -> Result<(), NormalizationError> {
        let N::Mapping(entries) = &self.input.nodes()[member] else {
            return Ok(());
        };
        let &[(key, value)] = entries.as_slice() else {
            return Ok(());
        };
        if !matches!(&self.input.nodes()[key], N::Text(name) if name == "path")
            || !matches!(self.input.nodes()[value], N::Text(_))
        {
            return Ok(());
        }
        self.finding(
            path,
            ConditionCode::SchemaRedundantLongForm,
            vec![
                ("field", C::Text("path".into())),
                ("shorthand", C::InputValue(value)),
            ],
        )
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
                return self.bad_type(node, C::LayerKind(DocumentKind::Sequence), path);
            };
            // Compare the retained identity occurrences directly.
            let mut seen: Vec<usize> = Vec::new();
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
                        if self.input.nodes()[prior] == self.input.nodes()[id] {
                            duplicate = true;
                            break;
                        }
                    }
                    if duplicate {
                        let logical = self.join(&logical, identity)?;
                        self.finding(
                            &logical,
                            ConditionCode::SchemaLayerIdentifierRepeated,
                            vec![("identifier", C::Text(label))],
                        )?;
                    }
                    seen.push(id);
                }
            }
        } else {
            let N::Mapping(entries) = &self.input.nodes()[node] else {
                return self.bad_type(node, C::LayerKind(DocumentKind::Mapping), path);
            };
            for &(key, member) in entries {
                let label = self.label(key)?;
                let logical = self.member_path(path, key, &label)?;
                let key_path = self.join(path, &format!("key({label})"))?;
                self.validate_type("identifier", key, &key_path)?;
                if kind.path_shorthand && matches!(self.input.nodes()[member], N::Text(_)) {
                    self.validate_type("project_path", member, &logical)?;
                } else {
                    let before = self.findings.len();
                    self.member(member, kind, &logical)?;
                    if kind.path_shorthand && self.findings.len() == before {
                        self.restated_path(member, &logical)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn validate(&mut self) -> Result<(), NormalizationError> {
        self.depth(0)?;
        let root = self.input.root();
        if !matches!(&self.input.nodes()[root], N::Mapping(entries) if !entries.is_empty()) {
            return self.bad_type(root, C::Text("root_class".into()), "$");
        }
        if self.field(root, "schema_version")?.is_none() {
            self.finding(
                "schema_version",
                ConditionCode::SchemaLayerVersionMismatch,
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
            if matches!(self.input.nodes()[value], N::Null) {
                if self.schema.descriptors()[field.descriptor]
                    .descriptor
                    .required()
                {
                    self.finding(
                        &field.name,
                        ConditionCode::SchemaLayerInvalidClear,
                        vec![("field", C::Text(field.name.clone()))],
                    )?;
                }
            } else if let Some(kind) = collection(self.schema, field) {
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
        if matches!(self.input.nodes()[node], N::Sequence(_) | N::Mapping(_)) {
            return Err(ValidationError::InvalidDescriptor.into());
        }

        Ok(self.push(self.input.nodes()[node].clone(), self.origin(node, false)))
    }

    fn import(&mut self, normalized: NormalizedDocument) -> Result<usize, NormalizationError> {
        let offset = self.nodes.len();
        for (node, origin) in normalized.document.nodes().iter().zip(normalized.origins) {
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
        let mut output = Vec::with_capacity(fields.len());
        for field in fields {
            let Some((key, value)) = self.field(node, &field.name)? else {
                continue;
            };
            let key = self.scalar(key)?;
            let result = if matches!(self.input.nodes()[value], N::Null) {
                self.scalar(value)?
            } else if let Some(collection) =
                collection(self.schema, field).filter(|_| kind.is_none())
            {
                self.normalize_members(value, collection)?
            } else {
                // Members keep clear markers at every depth; a whole root value
                // replaces what it inherits, so its nested nulls are absent.
                let (fragment, nulls) = if kind.is_some() {
                    (true, NullPolicy::Clear)
                } else {
                    (false, NullPolicy::Absent)
                };
                let normalized = self.schema.normalize_descriptor_with(
                    field.descriptor,
                    self.input,
                    value,
                    fragment,
                    nulls,
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
        let fields = kind.class.map(|class| class.fields.as_slice());
        if kind.identity.is_some() {
            let N::Sequence(items) = &self.input.nodes()[node] else {
                unreachable!()
            };
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
            // A custom identifier alias can admit e.g. 2 and "2". The host
            // stringifies input keys: keep the first position/key occurrence
            // and last member value. Select before copying so discarded values
            // never leave unreachable nodes in the owned output arena. Every
            // authored value has already been validated by the preceding pass.
            let mut selected: Vec<(usize, usize)> = Vec::with_capacity(entries.len());
            let mut positions: BTreeMap<String, usize> = BTreeMap::new();
            for &(key, member) in entries {
                let label = self.label(key)?;
                if let Some(&position) = positions.get(&label) {
                    selected[position].1 = member;
                } else {
                    positions.insert(label, selected.len());
                    selected.push((key, member));
                }
            }
            let mut output = Vec::with_capacity(selected.len());
            for (key, member) in selected {
                let key = if matches!(self.input.nodes()[key], N::Text(_)) {
                    self.scalar(key)?
                } else {
                    let label = self.label(key)?;
                    self.push(N::Text(label), self.origin(key, true))
                };
                let value =
                    if kind.path_shorthand && matches!(self.input.nodes()[member], N::Text(_)) {
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
    /// Admit and normalize one schema-shaped inheritance contribution (LAYER-4).
    /// All ordered field findings precede normalization; invalid input publishes no
    /// partial document. Parent traversal and role-specific version consistency remain
    /// caller responsibilities. Clear markers in root fields and keyed members, at
    /// every depth, survive until composition.
    pub fn normalize_layer(
        &self,
        input: &Document,
    ) -> Result<NormalizedDocument, NormalizationError> {
        let mut run = Run {
            schema: self,
            input,
            findings: Vec::new(),
            nodes: Vec::new(),
            origins: Vec::new(),
        };
        run.validate()?;
        if !run.findings.is_empty() {
            return Err(NormalizationError::Invalid(run.findings));
        }
        let root = run.normalize_fields(input.root(), &self.root_class().fields, None)?;
        let document = Document::new(run.nodes, root).map_err(NormalizationError::Document)?;
        Ok(NormalizedDocument {
            document,
            origins: run.origins,
        })
    }
}
