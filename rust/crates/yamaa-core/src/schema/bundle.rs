//! Complete decoded-bundle structure. Filesystem authority and YAML stay in adapters.
//! A structure is not executable: defaults and document values still need interpretation.

use super::{
    Descriptor, DescriptorError, DescriptorIssue, DescriptorSite, Document, DocumentNode as N,
};
use crate::language::LanguageVersion;
use std::{
    collections::{BTreeMap, BTreeSet},
    vec,
};

/// One immutable decoded source, identified by a bundle-local filename, never a digest.
#[derive(Clone, Debug)]
pub struct SchemaModule {
    pub name: String,
    pub document: Document,
}

/// Fixed semantic recursion guard, independent of input storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundleResource {
    Depth,
}

/// Structured schema defects retain provenance; host adapters render existing diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BundleIssueKind {
    ModuleName,
    DuplicateModule(String),
    ExpectedMapping,
    Version,
    VersionMismatch { expected: String, actual: String },
    IncludesList,
    UnsafeInclude { node: usize },
    MissingInclude(String),
    IncludeCycle(Vec<String>),
    UnusedModule,
    DeclarationName,
    UnknownDeclaration(String),
    DuplicateDeclaration(String),
    DuplicateRegistryEntry(String),
    MissingRoot(String),
    FieldsFrom,
    UnknownFieldsFrom(String),
    FieldsFromCycle(Vec<String>),
    FieldEntry,
    FieldName,
    DuplicateField(String),
    RegistryOnly,
    RegistryName,
    EmptyRegistry,
    UnreferencedRegistry,
    RegistryEntryName,
    RegistryEntryShape,
    UnknownRegistry(String),
    UnknownType(String),
    Descriptor(DescriptorIssue),
}

/// Module indices always refer to the caller's original source order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundleIssue {
    pub module: usize,
    pub path: String,
    pub kind: BundleIssueKind,
}

/// Refusals cannot be mistaken for successful schema admission or language findings.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BundleError {
    #[error("schema bundle invalid: {0:?}")]
    Invalid(Vec<BundleIssue>),
    #[error("schema bundle limit: resource={resource:?}, limit={limit:?}")]
    Limit {
        resource: BundleResource,
        limit: usize,
    },
    #[error("schema bundle descriptor: {0:?}")]
    Descriptor(DescriptorError),
}

/// A descriptor's defaults refer into its origin module, even after field reuse.
#[derive(Clone, Debug)]
pub struct LocatedDescriptor {
    pub module: usize,
    /// Original decoded descriptor occurrence, including after fields_from expansion.
    pub source_node: usize,
    pub path: String,
    pub descriptor: Descriptor,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaField {
    pub name: String,
    pub descriptor: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaClass {
    pub name: String,
    pub fields: Vec<SchemaField>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaAliasKind {
    Descriptor(usize),
    Registry(usize),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaAlias {
    pub name: String,
    pub kind: SchemaAliasKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaShape {
    Class(Vec<SchemaField>),
    Descriptor(usize),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaRegistryEntry {
    pub name: String,
    pub shape: SchemaShape,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaRegistry {
    pub name: String,
    pub entries: Vec<SchemaRegistryEntry>,
}

/// Structural admission and complete-bundle reference resolution; no host effects.
#[derive(Clone, Debug)]
pub struct SchemaStructure {
    /// Set only by successful default preparation; captured declarations cannot be mutated.
    pub(super) defaults_validated: bool,
    modules: Vec<SchemaModule>,
    version: String,
    root: usize,
    classes: Vec<SchemaClass>,
    aliases: Vec<SchemaAlias>,
    registries: Vec<SchemaRegistry>,
    descriptors: Vec<LocatedDescriptor>,
    class_indices: BTreeMap<String, usize>,
    alias_indices: BTreeMap<String, usize>,
    /// Fields a version view leaves out, by owning class or registry entry and
    /// field name, with the language version that introduced each (P-VER-1).
    later: BTreeMap<(String, String), LanguageVersion>,
}

#[derive(Clone, Copy, Debug)]
struct Raw {
    module: usize,
    node: usize,
}
#[derive(Clone)]
struct Named {
    name: String,
    raw: Raw,
}
#[derive(Clone)]
struct ExpandedFields {
    entries: Vec<Raw>,
    repeated: bool,
}
struct RawRegistry {
    name: String,
    module: usize,
    entries: Vec<(Raw, Raw)>,
}

struct Builder {
    modules: Vec<SchemaModule>,
    descriptors: Vec<LocatedDescriptor>,
    issues: Vec<BundleIssue>,
    references: Vec<(Raw, String, String)>,
}

/// Includes admit only the contract's basename grammar, independent of host path rules.
pub fn safe_include(name: &str) -> bool {
    name.strip_prefix("schema_")
        .and_then(|s| s.strip_suffix(".yaml"))
        .is_some_and(|middle| {
            !middle.is_empty()
                && middle
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
}

/// Builtins take precedence over named declarations, matching the schema type vocabulary.
fn builtin(name: &str) -> bool {
    matches!(
        name,
        "str" | "int" | "float" | "bool" | "null" | "list" | "dict"
    )
}

impl Builder {
    /// Borrow a node from a previously admitted immutable document.
    fn node(&self, raw: Raw) -> &N {
        &self.modules[raw.module].document.nodes()[raw.node]
    }
    /// Find a descriptor member in its retained declaring document.
    fn field(&mut self, raw: Raw, name: &str) -> Result<Option<Raw>, BundleError> {
        Ok(self.modules[raw.module]
            .document
            .field(raw.node, name)
            .map(|node| Raw {
                module: raw.module,
                node,
            }))
    }
    /// Append a structured finding without discarding its declaring module or path.
    fn issue(&mut self, raw: Raw, path: &str, kind: BundleIssueKind) -> Result<(), BundleError> {
        self.issues.push(BundleIssue {
            module: raw.module,
            path: path.into(),
            kind,
        });
        Ok(())
    }
    /// Stop an early declaration/graph failure, preserving any accumulated findings.
    fn fail(&mut self, raw: Raw, path: &str, kind: BundleIssueKind) -> BundleError {
        match self.issue(raw, path, kind) {
            Err(error) => error,
            Ok(()) => BundleError::Invalid(std::mem::take(&mut self.issues)),
        }
    }
    /// Compile field/alias/registry descriptors in the existing stage order.
    fn descriptor(
        &mut self,
        raw: Raw,
        path: &str,
        site: DescriptorSite,
    ) -> Result<Option<usize>, BundleError> {
        let report = Descriptor::admit(&self.modules[raw.module].document, raw.node, site)
            .map_err(BundleError::Descriptor)?;
        for finding in report.issues() {
            self.issue(raw, path, BundleIssueKind::Descriptor(finding.clone()))?;
        }
        for reference in report.references() {
            self.references.push((raw, path.into(), reference.clone()));
        }
        Ok(report.into_descriptor().map(|descriptor| {
            let index = self.descriptors.len();
            self.descriptors.push(LocatedDescriptor {
                module: raw.module,
                source_node: raw.node,
                path: path.into(),
                descriptor,
            });
            index
        }))
    }
    /// Validate class entries after fields_from has expanded; preserve schema field order.
    fn fields(&mut self, entries: &[Raw], path: &str) -> Result<Vec<SchemaField>, BundleError> {
        let mut fields = Vec::new();
        let mut names = BTreeSet::new();
        for (index, &raw) in entries.iter().enumerate() {
            let N::Mapping(items) = self.node(raw) else {
                self.issue(
                    raw,
                    &format!("{path}[{index}]"),
                    BundleIssueKind::FieldEntry,
                )?;
                continue;
            };
            if items.len() != 1 {
                self.issue(
                    raw,
                    &format!("{path}[{index}]"),
                    BundleIssueKind::FieldEntry,
                )?;
                continue;
            }
            let (name, descriptor) = items[0];
            let N::Text(name) = self.node(Raw {
                module: raw.module,
                node: name,
            }) else {
                self.issue(raw, &format!("{path}[{index}]"), BundleIssueKind::FieldName)?;
                continue;
            };
            if name.is_empty() {
                self.issue(raw, &format!("{path}[{index}]"), BundleIssueKind::FieldName)?;
                continue;
            }
            let name = name.clone();
            let field_path = format!("{path}.{name}");
            if !names.insert(name.clone()) {
                self.issue(
                    raw,
                    &field_path,
                    BundleIssueKind::DuplicateField(name.clone()),
                )?;
            }
            if let Some(descriptor) = self.descriptor(
                Raw {
                    module: raw.module,
                    node: descriptor,
                },
                &field_path,
                DescriptorSite::ClassField,
            )? {
                fields.push(SchemaField { name, descriptor });
            }
        }
        Ok(fields)
    }
    /// Expand inherited immutable field occurrences in their declaring order.
    fn expand(
        &mut self,
        index: usize,
        classes: &[Named],
        names: &BTreeMap<String, usize>,
        expanded: &mut [Option<ExpandedFields>],
        active: &mut Vec<usize>,
    ) -> Result<ExpandedFields, BundleError> {
        if active.contains(&index) {
            let mut cycle: Vec<_> = active.iter().map(|&id| classes[id].name.clone()).collect();
            cycle.push(classes[index].name.clone());
            return Err(self.fail(
                classes[index].raw,
                &classes[index].name,
                BundleIssueKind::FieldsFromCycle(cycle),
            ));
        }
        if let Some(fields) = &expanded[index] {
            return Ok(fields.clone());
        }
        if active.len() >= crate::limits::Limits::SEMANTIC_DEPTH {
            return Err(BundleError::Limit {
                resource: BundleResource::Depth,
                limit: crate::limits::Limits::SEMANTIC_DEPTH,
            });
        }
        active.push(index);
        let raw = classes[index].raw;
        let N::Sequence(entries) = self.node(raw) else {
            unreachable!("class classified at collection");
        };
        let entries = entries.clone();
        let mut result = Vec::new();
        let mut occurrences = BTreeSet::new();
        let mut repeated = false;
        for node in entries {
            let item = Raw {
                module: raw.module,
                node,
            };
            if let Some(from) = self.field(item, "fields_from")? {
                let N::Mapping(fields) = self.node(item) else {
                    unreachable!()
                };
                let N::Text(name) = self.node(from) else {
                    return Err(self.fail(item, &classes[index].name, BundleIssueKind::FieldsFrom));
                };
                if fields.len() != 1 || name.is_empty() {
                    return Err(self.fail(item, &classes[index].name, BundleIssueKind::FieldsFrom));
                }
                let name = name.clone();
                let Some(&target) = names.get(&name) else {
                    return Err(self.fail(
                        item,
                        &classes[index].name,
                        BundleIssueKind::UnknownFieldsFrom(name),
                    ));
                };
                let copied = self.expand(target, classes, names, expanded, active)?;
                if !repeated {
                    for field in copied.entries {
                        result.push(field);
                        if !occurrences.insert((field.module, field.node)) {
                            repeated = true;
                            break;
                        }
                    }
                }
            } else if !repeated {
                result.push(item);
                if !occurrences.insert((item.module, item.node)) {
                    repeated = true;
                }
            }
        }
        active.pop();
        // A repeated leaf necessarily repeats a field name or shape defect.
        // Keep the prefix through that occurrence so the normal descriptor pass
        // reports any earlier defect first, at the selected class's paths. Later
        // reuse edges still receive complete structural validation. Cache the
        // bounded prefix too, so invalid reuse DAGs cannot multiply traversals.
        let result = ExpandedFields {
            entries: result,
            repeated,
        };
        expanded[index] = Some(result.clone());
        Ok(result)
    }
}

struct CollectedDeclarations {
    version: Option<String>,
    kinds: BTreeMap<String, i32>,
    raw_classes: Vec<Named>,
    raw_aliases: Vec<Named>,
    raw_registries: Vec<RawRegistry>,
}

impl Builder {
    /// Traverse the include closure and collect raw declarations in stage order.
    fn collect(
        &mut self,
        entry: usize,
        module_names: &BTreeMap<String, usize>,
    ) -> Result<CollectedDeclarations, BundleError> {
        let mut completed = BTreeSet::new();
        let mut pending = vec![(entry, Vec::<usize>::new())];
        let mut version: Option<String> = None;
        let mut kinds = BTreeMap::new();
        let mut raw_classes = Vec::new();
        let mut raw_aliases = Vec::new();
        let mut raw_registries: Vec<RawRegistry> = Vec::new();
        while let Some((module, ancestors)) = pending.pop() {
            let raw = Raw {
                module,
                node: self.modules[module].document.root(),
            };
            if ancestors.contains(&module) {
                let mut cycle: Vec<_> = ancestors
                    .iter()
                    .map(|&i| self.modules[i].name.clone())
                    .collect();
                cycle.push(self.modules[module].name.clone());
                return Err(self.fail(raw, "$", BundleIssueKind::IncludeCycle(cycle)));
            }
            if !completed.insert(module) {
                continue;
            }
            if ancestors.len() >= crate::limits::Limits::SEMANTIC_DEPTH {
                return Err(BundleError::Limit {
                    resource: BundleResource::Depth,
                    limit: crate::limits::Limits::SEMANTIC_DEPTH,
                });
            }
            let N::Mapping(entries) = self.node(raw) else {
                return Err(self.fail(raw, "$", BundleIssueKind::ExpectedMapping));
            };
            let entries = entries.clone();
            let Some(v) = self.field(raw, "version")? else {
                return Err(self.fail(raw, "$", BundleIssueKind::Version));
            };
            let N::Text(v) = self.node(v) else {
                return Err(self.fail(raw, "$", BundleIssueKind::Version));
            };
            if v.is_empty() {
                return Err(self.fail(raw, "$", BundleIssueKind::Version));
            }
            let v = v.clone();
            if let Some(expected) = &version {
                if *expected != v {
                    return Err(self.fail(
                        raw,
                        "$",
                        BundleIssueKind::VersionMismatch {
                            expected: expected.clone(),
                            actual: v,
                        },
                    ));
                }
            } else {
                version = Some(v);
            }
            if let Some(includes) = self.field(raw, "includes")? {
                let N::Sequence(items) = self.node(includes) else {
                    return Err(self.fail(raw, "includes", BundleIssueKind::IncludesList));
                };
                let items = items.clone();
                for node in items {
                    let N::Text(name) = self.node(Raw { module, node }) else {
                        return Err(self.fail(
                            raw,
                            "includes",
                            BundleIssueKind::UnsafeInclude { node },
                        ));
                    };
                    if !safe_include(name) {
                        return Err(self.fail(
                            raw,
                            "includes",
                            BundleIssueKind::UnsafeInclude { node },
                        ));
                    }
                    let Some(&target) = module_names.get(name) else {
                        let name = name.clone();
                        return Err(self.fail(
                            raw,
                            "includes",
                            BundleIssueKind::MissingInclude(name),
                        ));
                    };
                    let mut chain = ancestors.clone();
                    chain.push(module);
                    pending.push((target, chain));
                }
            }
            for (key, node) in entries {
                let N::Text(name) = self.node(Raw { module, node: key }) else {
                    return Err(self.fail(raw, "$", BundleIssueKind::DeclarationName));
                };
                if matches!(name.as_str(), "version" | "includes") {
                    continue;
                }
                if name.is_empty() {
                    return Err(self.fail(raw, "$", BundleIssueKind::DeclarationName));
                }
                let name = name.clone();
                let value = Raw { module, node };
                let kind = match self.node(value) {
                    N::Sequence(_) => 0,
                    N::Mapping(_) => {
                        let type_value = self.field(value, "type")?;
                        let registry = self.field(value, "registry")?;
                        let alias = type_value.is_some_and(|r| match self.node(r) {
                            N::Text(_) => true,
                            N::Sequence(items) => items.iter().all(|&id| {
                                matches!(self.modules[module].document.nodes()[id], N::Text(_))
                            }),
                            _ => false,
                        }) || registry
                            .is_some_and(|r| matches!(self.node(r), N::Text(_)));
                        if alias {
                            1
                        } else {
                            2
                        }
                    }
                    _ => {
                        return Err(self.fail(
                            value,
                            &name,
                            BundleIssueKind::UnknownDeclaration(name.clone()),
                        ))
                    }
                };
                if let Some(&previous) = kinds.get(&name) {
                    if previous != 2 || kind != 2 {
                        return Err(self.fail(
                            value,
                            &name,
                            BundleIssueKind::DuplicateDeclaration(name.clone()),
                        ));
                    }
                }
                kinds.insert(name.clone(), kind);
                match kind {
                    0 => raw_classes.push(Named { name, raw: value }),
                    1 => raw_aliases.push(Named { name, raw: value }),
                    _ => {
                        let index = raw_registries
                            .iter()
                            .position(|r| r.name == name)
                            .unwrap_or_else(|| {
                                let i = raw_registries.len();
                                raw_registries.push(RawRegistry {
                                    name: name.clone(),
                                    module,
                                    entries: Vec::new(),
                                });
                                i
                            });
                        let N::Mapping(items) = self.node(value) else {
                            unreachable!()
                        };
                        let items = items.clone();
                        for (key, payload) in items {
                            let key = Raw { module, node: key };
                            let payload = Raw {
                                module,
                                node: payload,
                            };
                            for &(previous, _) in &raw_registries[index].entries {
                                if super::document::scalar_keys_equal(
                                    self.node(previous),
                                    self.node(key),
                                ) {
                                    let label = match self.node(key) {
                                        N::Text(text) => text.clone(),
                                        _ => "<non-text>".into(),
                                    };
                                    return Err(self.fail(
                                        value,
                                        &name,
                                        BundleIssueKind::DuplicateRegistryEntry(label),
                                    ));
                                }
                            }
                            raw_registries[index].entries.push((key, payload));
                        }
                    }
                }
            }
        }
        for module in 0..self.modules.len() {
            if !completed.contains(&module) {
                let raw = Raw {
                    module,
                    node: self.modules[module].document.root(),
                };
                return Err(self.fail(raw, "$", BundleIssueKind::UnusedModule));
            }
        }
        Ok(CollectedDeclarations {
            version,
            kinds,
            raw_classes,
            raw_aliases,
            raw_registries,
        })
    }
}

impl SchemaStructure {
    /// Load the complete provided include closure, then resolve structural references.
    /// The caller must enforce filesystem/symlink authority before supplying decoded modules.
    pub fn admit(
        modules: Vec<SchemaModule>,
        entry: usize,
        root_name: &str,
    ) -> Result<Self, BundleError> {
        if modules.is_empty() || entry >= modules.len() {
            return Err(BundleError::Invalid(vec![BundleIssue {
                module: entry,
                path: "$".into(),
                kind: BundleIssueKind::ModuleName,
            }]));
        }
        let mut module_names = BTreeMap::new();
        for (index, module) in modules.iter().enumerate() {
            if module.name.is_empty() || module.name.contains(['/', '\\']) {
                return Err(BundleError::Invalid(vec![BundleIssue {
                    module: index,
                    path: "$".into(),
                    kind: BundleIssueKind::ModuleName,
                }]));
            }
            if module_names.insert(module.name.clone(), index).is_some() {
                return Err(BundleError::Invalid(vec![BundleIssue {
                    module: index,
                    path: "$".into(),
                    kind: BundleIssueKind::DuplicateModule(module.name.clone()),
                }]));
            }
        }
        let mut builder = Builder {
            modules,
            descriptors: Vec::new(),
            issues: Vec::new(),
            references: Vec::new(),
        };
        let CollectedDeclarations {
            version,
            kinds,
            raw_classes,
            raw_aliases,
            raw_registries,
        } = builder.collect(entry, &module_names)?;
        let class_names: BTreeMap<_, _> = raw_classes
            .iter()
            .enumerate()
            .map(|(i, c)| (c.name.clone(), i))
            .collect();
        let Some(&root) = class_names.get(root_name) else {
            let raw = Raw {
                module: entry,
                node: builder.modules[entry].document.root(),
            };
            return Err(builder.fail(raw, "$", BundleIssueKind::MissingRoot(root_name.into())));
        };
        let mut expanded = vec![None; raw_classes.len()];
        let mut classes = Vec::new();
        for (index, class) in raw_classes.iter().enumerate() {
            let expanded = builder.expand(
                index,
                &raw_classes,
                &class_names,
                &mut expanded,
                &mut Vec::new(),
            )?;
            let fields = builder.fields(&expanded.entries, &class.name)?;
            if expanded.repeated {
                return Err(BundleError::Invalid(std::mem::take(&mut builder.issues)));
            }
            classes.push(SchemaClass {
                name: class.name.clone(),
                fields,
            });
        }
        let registry_names: BTreeMap<_, _> = raw_registries
            .iter()
            .enumerate()
            .map(|(i, r)| (r.name.clone(), i))
            .collect();
        let mut referenced = BTreeSet::new();
        let mut aliases = Vec::new();
        for alias in &raw_aliases {
            if let Some(registry) = builder.field(alias.raw, "registry")? {
                let N::Mapping(items) = builder.node(alias.raw) else {
                    unreachable!()
                };
                if items.len() != 1 {
                    builder.issue(alias.raw, &alias.name, BundleIssueKind::RegistryOnly)?;
                }
                let name = match builder.node(registry) {
                    N::Text(name) if !name.is_empty() => name.clone(),
                    _ => {
                        builder.issue(alias.raw, &alias.name, BundleIssueKind::RegistryName)?;
                        continue;
                    }
                };
                referenced.insert(name.clone());
                if let Some(&id) = registry_names.get(&name) {
                    aliases.push(SchemaAlias {
                        name: alias.name.clone(),
                        kind: SchemaAliasKind::Registry(id),
                    });
                }
            } else if let Some(id) =
                builder.descriptor(alias.raw, &alias.name, DescriptorSite::NamedType)?
            {
                aliases.push(SchemaAlias {
                    name: alias.name.clone(),
                    kind: SchemaAliasKind::Descriptor(id),
                });
            }
        }
        let mut registries = Vec::new();
        for registry in &raw_registries {
            let raw = Raw {
                module: registry.module,
                node: builder.modules[registry.module].document.root(),
            };
            if registry.entries.is_empty() {
                builder.issue(raw, &registry.name, BundleIssueKind::EmptyRegistry)?;
            }
            if !referenced.contains(&registry.name) {
                builder.issue(raw, &registry.name, BundleIssueKind::UnreferencedRegistry)?;
            }
            let mut entries = Vec::new();
            for &(key, payload) in &registry.entries {
                let name = match builder.node(key) {
                    N::Text(name) if !name.is_empty() => name.clone(),
                    _ => {
                        builder.issue(key, &registry.name, BundleIssueKind::RegistryEntryName)?;
                        continue;
                    }
                };
                let path = format!("{}.{name}", registry.name);
                let shape = match builder.node(payload) {
                    N::Sequence(items) => {
                        let raw: Vec<_> = items
                            .iter()
                            .map(|&node| Raw {
                                module: payload.module,
                                node,
                            })
                            .collect();
                        Some(SchemaShape::Class(builder.fields(&raw, &path)?))
                    }
                    N::Mapping(_) => builder
                        .descriptor(payload, &path, DescriptorSite::RegistryEntry)?
                        .map(SchemaShape::Descriptor),
                    _ => {
                        builder.issue(payload, &path, BundleIssueKind::RegistryEntryShape)?;
                        None
                    }
                };
                if let Some(shape) = shape {
                    entries.push(SchemaRegistryEntry { name, shape });
                }
            }
            registries.push(SchemaRegistry {
                name: registry.name.clone(),
                entries,
            });
        }
        for name in referenced {
            if !registry_names.contains_key(&name) {
                let raw = Raw {
                    module: entry,
                    node: builder.modules[entry].document.root(),
                };
                builder.issue(raw, "$", BundleIssueKind::UnknownRegistry(name))?;
            }
        }
        for (raw, path, reference) in std::mem::take(&mut builder.references) {
            if !builtin(&reference)
                && !class_names.contains_key(&reference)
                && kinds.get(&reference) != Some(&1)
            {
                builder.issue(raw, &path, BundleIssueKind::UnknownType(reference))?;
            }
        }
        if !builder.issues.is_empty() {
            return Err(BundleError::Invalid(builder.issues));
        }
        let alias_indices = aliases
            .iter()
            .enumerate()
            .map(|(index, alias)| (alias.name.clone(), index))
            .collect();
        Ok(Self {
            defaults_validated: false,
            modules: builder.modules,
            version: version.expect("nonempty closure"),
            root,
            classes,
            aliases,
            registries,
            descriptors: builder.descriptors,
            class_indices: class_names,
            alias_indices,
            later: BTreeMap::new(),
        })
    }

    /// The view of this closure a document declaring `version` is checked
    /// against (P-VER-1): a field whose `since` names a later version is not
    /// declared, and writing it fails as `unknown_field`, with context naming
    /// that version. Descriptor indices are shared with this closure, so
    /// findings made against the view resolve against either.
    pub fn at_version(&self, version: LanguageVersion) -> Self {
        let mut view = self.clone();
        let descriptors = &self.descriptors;
        let mut later = std::mem::take(&mut view.later);
        let mut keep = |owner: &str, field: &SchemaField| {
            let since = descriptors[field.descriptor]
                .descriptor
                .since()
                .filter(|since| *since > version);
            if let Some(since) = since {
                later.insert((owner.into(), field.name.clone()), since);
            }
            since.is_none()
        };
        for class in &mut view.classes {
            class.fields.retain(|field| keep(&class.name, field));
        }
        for registry in &mut view.registries {
            for entry in &mut registry.entries {
                if let SchemaShape::Class(fields) = &mut entry.shape {
                    fields.retain(|field| keep(&entry.name, field));
                }
            }
        }
        view.later = later;
        view
    }
    /// The language version that introduced `field` of `owner`, a class or a
    /// class-shaped registry entry, when this view leaves it out as newer than
    /// the version it checks.
    pub fn later_field(&self, owner: &str, field: &str) -> Option<LanguageVersion> {
        self.later.get(&(owner.into(), field.into())).copied()
    }

    /// Decoded schema documents remain alive for defaults and source-aware diagnostics.
    pub fn modules(&self) -> &[SchemaModule] {
        &self.modules
    }
    /// Every module has the same nonempty version after structure admission.
    pub fn version(&self) -> &str {
        &self.version
    }
    /// Return the explicitly selected entry-point class.
    pub fn root_class(&self) -> &SchemaClass {
        &self.classes[self.root]
    }
    /// Classes retain field order after immutable reuse expansion.
    pub fn classes(&self) -> &[SchemaClass] {
        &self.classes
    }
    /// Recursive aliases retain their named references for value-specific cycle guards.
    pub fn aliases(&self) -> &[SchemaAlias] {
        &self.aliases
    }
    /// Registry contributions retain complete-bundle declaration order.
    pub fn registries(&self) -> &[SchemaRegistry] {
        &self.registries
    }
    /// Inspect descriptor origins before validating defaults or document values.
    pub fn descriptors(&self) -> &[LocatedDescriptor] {
        &self.descriptors
    }
    /// Bind class references without repeatedly scanning all declarations.
    pub fn class_named(&self, name: &str) -> Option<&SchemaClass> {
        self.class_indices
            .get(name)
            .map(|&index| &self.classes[index])
    }
    /// Return a stable alias identity for value-specific recursive-reference guards.
    pub fn alias_named(&self, name: &str) -> Option<(usize, &SchemaAlias)> {
        self.alias_indices
            .get(name)
            .map(|&index| (index, &self.aliases[index]))
    }
}
