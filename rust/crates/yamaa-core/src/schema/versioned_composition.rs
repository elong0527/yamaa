//! Composition of the normalized layers of a 1.x specification (LAYER-3 to
//! LAYER-9, P-CLR-1).
//!
//! Layers compose root ancestor first. Before a layer's own fields compose,
//! each path its `clear` lists removes the value the layer inherits there;
//! the layer's fields then compose as a 0.5 layer's do (LAYER-5, LAYER-6),
//! except that YAML null is the missing value and composes as the value it
//! is, never removing anything (P-NULL-1). `parent` and `clear` are consumed
//! and never contributed.

use super::{
    document::scalar_keys_equal,
    layers::{self, ComposedLayers},
    spec_path::SpecPath,
    validation::{matches_member, shorthand_field, NullPolicy},
    versioned_layer::{invalid_clear, IDENTITIES},
    CompositionOrigin, Document, DocumentNode as N, NormalizationError, NormalizedDocument,
    SchemaAliasKind, SchemaClass, SchemaDiagnostic, SchemaField, SchemaShape, SchemaSource,
    SchemaStructure, TypeExpression, TypeNode,
};
use std::collections::BTreeSet;

/// Why the layers of a 1.x specification do not compose.
#[derive(Clone, Debug, PartialEq)]
pub enum VersionedCompositionError {
    /// The layer numbered `layer` in contribution order lists in `clear` a
    /// value it cannot clear: findings in that layer, each `invalid_clear` at
    /// its `clear[i]` (LAYER-9).
    Clear {
        layer: usize,
        findings: Vec<SchemaDiagnostic>,
    },
    /// The normalized layers are not shaped as their schema admits.
    Normalization(NormalizationError),
}

impl From<NormalizationError> for VersionedCompositionError {
    fn from(error: NormalizationError) -> Self {
        Self::Normalization(error)
    }
}

impl SchemaStructure {
    /// Compose the normalized layers of a 1.x specification, root ancestor
    /// first, each normalized by [`SchemaStructure::normalize_versioned_layer`]
    /// against the view of the version it declares. A layer's `clear` paths
    /// remove the values it inherits before its own fields compose, so
    /// clearing a mapping and writing it in one layer replaces it (LAYER-9).
    /// No defaults are materialized here: the resolved specification's
    /// normalization fills each field the composed document lacks, a cleared
    /// one included (LAYER-7). Provenance names keyed members by identity, in
    /// the spec path notation (P-CLR-2).
    pub fn compose_versioned_layers(
        &self,
        layers: &[NormalizedDocument],
    ) -> Result<ComposedLayers, VersionedCompositionError> {
        let (mut document, mut origins) = layers::empty()?;
        let mut diagnostics = Vec::new();
        for (layer, input) in layers.iter().enumerate() {
            (document, origins) = Clearing {
                schema: self,
                layers,
                document: &document,
                origins: &origins,
            }
            .apply(layer, &input.document)?;
            (document, origins) = layers::compose_step(
                self,
                NullPolicy::Value,
                &mut diagnostics,
                (&document, &origins),
                (layer, &input.document),
            )?;
        }
        let shorthand = origins
            .iter()
            .map(|&origin| shorthand_key(layers, origin))
            .collect::<Vec<_>>();
        let transparent = |key: usize| shorthand[key];
        Ok(layers::finish(
            document,
            origins,
            diagnostics,
            NullPolicy::Value,
            &transparent,
        )?)
    }
}

/// Whether the composed node with this origin is a key that shorthand
/// expansion wrote in its layer, such as the `value` of a handled expression
/// written bare: a 1.x path names the value through it without spelling it
/// (P-CLR-1).
pub(crate) fn shorthand_key(
    layers: &[NormalizedDocument],
    origin: Option<CompositionOrigin>,
) -> bool {
    origin.is_some_and(|origin| {
        let written = layers[origin.input].origins[origin.node];
        written.generated && matches!(written.source, SchemaSource::Input)
    })
}

/// One step of a finding's path as the validator writes it in 1.x notation.
enum Step<'p> {
    /// `name` or `.name`: a bare key, or a list member's identity.
    Bare(&'p str),
    /// `['key']`: a quoted key.
    Quoted(String),
    /// `[label]`: a list index, or an integer or Boolean key.
    Bracket(&'p str),
}

/// The steps of `path`, each with its written text; `None` for a path this
/// reader does not split, such as `$`.
fn steps(path: &str) -> Option<Vec<(Step<'_>, &str)>> {
    let bytes = path.as_bytes();
    let mut steps = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        if bytes[at] == b'[' && bytes.get(at + 1) == Some(&b'\'') {
            let mut key = String::new();
            let mut index = at + 2;
            loop {
                let offset = path[index..].find('\'')?;
                key.push_str(&path[index..index + offset]);
                index += offset + 1;
                if bytes.get(index) == Some(&b'\'') {
                    key.push('\'');
                    index += 1;
                } else {
                    break;
                }
            }
            if bytes.get(index) != Some(&b']') {
                return None;
            }
            at = index + 1;
            steps.push((Step::Quoted(key), &path[start..at]));
        } else if bytes[at] == b'[' {
            let end = at + path[at..].find(']')?;
            at = end + 1;
            steps.push((Step::Bracket(&path[start + 1..end]), &path[start..at]));
        } else {
            let from = if bytes[at] == b'.' { at + 1 } else { at };
            if from == at && at != 0 {
                return None;
            }
            let end = path[from..]
                .find(['.', '['])
                .map_or(path.len(), |offset| from + offset);
            if end == from {
                return None;
            }
            at = end;
            steps.push((Step::Bare(&path[from..end]), &path[start..at]));
        }
    }
    Some(steps)
}

/// A finding's `path` through the composed `document`, written as the spec
/// path notation names it: a key `transparent` reports as written by
/// shorthand expansion is left out, so a finding inside a value written bare
/// names it through its shorthand, as its layer wrote it (P-CLR-2). Steps
/// beyond the composed values are kept as written.
pub(crate) fn written_path(
    document: &Document,
    transparent: &dyn Fn(usize) -> bool,
    path: &str,
) -> String {
    let Some(steps) = steps(path) else {
        return path.into();
    };
    let mut written = String::new();
    let mut node = Some(document.root());
    for (step, text) in steps {
        let name = match &step {
            Step::Bare(name) => Some(*name),
            Step::Quoted(name) => Some(name.as_str()),
            Step::Bracket(_) => None,
        };
        let found = node.and_then(|at| match (&document.nodes()[at], name, &step) {
            (N::Mapping(items), Some(name), _) => items
                .iter()
                .find(|&&(key, _)| matches!(&document.nodes()[key], N::Text(key) if key == name))
                .map(|&(key, value)| (Some(key), value)),
            // A list member is named by its identity, or by its index.
            (N::Sequence(items), Some(name), _) => items
                .iter()
                .find(|&&item| {
                    ["name", "id"].iter().any(|field| {
                        matches!(
                            document.field(item, field).map(|id| &document.nodes()[id]),
                            Some(N::Text(identity)) if identity == name
                        )
                    })
                })
                .map(|&item| (None, item)),
            (N::Sequence(items), None, Step::Bracket(index)) => index
                .parse::<usize>()
                .ok()
                .and_then(|index| items.get(index))
                .map(|&item| (None, item)),
            _ => None,
        });
        node = found.map(|(_, value)| value);
        if found.is_some_and(|(key, _)| key.is_some_and(transparent)) {
            continue;
        }
        if written.is_empty() {
            written.push_str(text.strip_prefix('.').unwrap_or(text));
        } else if text.starts_with(['.', '[']) {
            written.push_str(text);
        } else {
            written.push('.');
            written.push_str(text);
        }
    }
    written
}

/// What the value at a step of a clear path is expected to be.
#[derive(Clone, Copy)]
enum Expected<'s> {
    /// A field of this descriptor: one of its union members.
    Descriptor(usize),
    /// One type of a union, and the union's members it came from, whose
    /// shorthand a clear path may name its value through.
    Type {
        expression: &'s TypeExpression,
        node: usize,
        union: &'s [TypeExpression],
    },
    /// The fields of a class, or of a class-shaped registry payload.
    Fields {
        fields: &'s [SchemaField],
        class: Option<&'s SchemaClass>,
        union: &'s [TypeExpression],
    },
    /// A registry value: a mapping holding one keyword and its payload.
    Registry(usize),
    /// A `dict[K, V]` value, whose keys are written by the layers.
    Dictionary {
        expression: &'s TypeExpression,
        value: usize,
    },
    /// A scalar or a list: nothing in it is addressed by a clear path.
    Leaf,
}

/// The value a clear path names: the mapping entry holding it, and whether
/// removing it is refused.
#[derive(Clone, Copy)]
struct Located {
    mapping: usize,
    entry: usize,
    /// A required field, a registry keyword, or a keyed member itself.
    kept: bool,
}

struct Clearing<'a> {
    schema: &'a SchemaStructure,
    layers: &'a [NormalizedDocument],
    document: &'a Document,
    origins: &'a [Option<CompositionOrigin>],
}

/// The member type of a keyed collection field, a `list[T]` or `dict[K, T]`.
fn member_type(schema: &SchemaStructure, collection: usize) -> Option<(&TypeExpression, usize)> {
    let [expression] = schema.descriptors().get(collection)?.descriptor.members() else {
        return None;
    };
    match &expression.nodes()[expression.root()] {
        TypeNode::List(inner) => Some((expression, *inner)),
        TypeNode::Dictionary { value, .. } => Some((expression, *value)),
        TypeNode::Name(_) => None,
    }
}

impl<'a> Clearing<'a> {
    /// Remove what each path of `input`'s `clear` names from the accumulated
    /// value. Every entry is checked before anything is removed; any fault
    /// stops composition with the layer's findings, in list order.
    fn apply(
        &self,
        layer: usize,
        input: &Document,
    ) -> Result<(Document, Vec<Option<CompositionOrigin>>), VersionedCompositionError> {
        let entries = match input
            .field(input.root(), "clear")
            .map(|list| &input.nodes()[list])
        {
            Some(N::Sequence(items)) => items.as_slice(),
            _ => &[],
        };
        let mut findings = Vec::new();
        let mut removed = BTreeSet::new();
        for (index, &item) in entries.iter().enumerate() {
            let N::Text(written) = &input.nodes()[item] else {
                findings.push(invalid_clear(index, "clear"));
                continue;
            };
            let Some(path) = SpecPath::parse(written) else {
                findings.push(invalid_clear(index, written));
                continue;
            };
            match self.locate(&path)? {
                Some(located) if !located.kept => {
                    removed.insert((located.mapping, located.entry));
                }
                _ => findings.push(invalid_clear(index, path.last())),
            }
        }
        if !findings.is_empty() {
            return Err(VersionedCompositionError::Clear { layer, findings });
        }
        let mut copy = Copy {
            document: self.document,
            origins: self.origins,
            removed: &removed,
            nodes: Vec::new(),
            kept: Vec::new(),
        };
        let root = copy.node(self.document.root(), 0)?;
        let document = Document::new(copy.nodes, root).map_err(NormalizationError::Document)?;
        Ok((document, copy.kept))
    }

    /// The entry of `mapping` whose key is `key`, if any.
    fn entry(&self, mapping: usize, key: &str) -> Option<(usize, usize)> {
        let N::Mapping(items) = &self.document.nodes()[mapping] else {
            return None;
        };
        let wanted = N::Text(key.into());
        items
            .iter()
            .enumerate()
            .find(|(_, &(candidate, _))| {
                scalar_keys_equal(&wanted, &self.document.nodes()[candidate])
            })
            .map(|(index, &(_, value))| (index, value))
    }

    /// Whether the value at `node` came from a schema default rather than a
    /// layer: nothing a layer inherits.
    fn defaulted(&self, node: usize) -> bool {
        self.origins[node].is_none_or(|origin| {
            matches!(
                self.layers[origin.input].origins[origin.node].source,
                SchemaSource::Default { .. }
            )
        })
    }

    /// The value `path` names in the accumulated value, or `None` when it
    /// names nothing the layer inherits. A keyed member is addressed by its
    /// identity, and a value written through a shorthand union by the
    /// spelling its shorthand gives it (P-CLR-1).
    fn locate(&self, path: &SpecPath) -> Result<Option<Located>, NormalizationError> {
        let schema: &'a SchemaStructure = self.schema;
        let keys = path.keys();
        let root = self.document.root();
        let Some(field) = schema
            .root_class()
            .fields
            .iter()
            .find(|field| field.name == keys[0])
        else {
            return Ok(None);
        };
        let Some((entry, mut node)) = self.entry(root, &keys[0]) else {
            return Ok(None);
        };
        let mut located = Located {
            mapping: root,
            entry,
            kept: schema.descriptors()[field.descriptor].descriptor.required(),
        };
        let mut expected: Expected<'a> = Expected::Descriptor(field.descriptor);
        let mut rest = &keys[1..];
        // A keyed list member is addressed by its identity, and a member of a
        // keyed mapping by its key. The member itself stays: a path that
        // ends at it is refused.
        let identity = IDENTITIES
            .iter()
            .find(|(collection, _)| *collection == keys[0])
            .map(|&(_, identity)| identity);
        if identity.is_some() || matches!(keys[0].as_str(), "input" | "windows") {
            let Some((member, after)) = rest.split_first() else {
                return Ok(Some(located));
            };
            let Some((expression, member_node)) = member_type(schema, field.descriptor) else {
                return Ok(None);
            };
            let item = match identity {
                Some(identity) => {
                    let N::Sequence(items) = &self.document.nodes()[node] else {
                        return Ok(None);
                    };
                    let Some(&item) = items.iter().find(|&&item| {
                        matches!(
                            self.document.field(item, identity).map(|id| &self.document.nodes()[id]),
                            Some(N::Text(name)) if name == member
                        )
                    }) else {
                        return Ok(None);
                    };
                    located.kept = true;
                    item
                }
                None => {
                    let Some((entry, value)) = self.entry(node, member) else {
                        return Ok(None);
                    };
                    located = Located {
                        mapping: node,
                        entry,
                        kept: true,
                    };
                    value
                }
            };
            if after.is_empty() {
                return Ok(Some(located));
            }
            node = item;
            expected = Expected::Type {
                expression,
                node: member_node,
                union: &[],
            };
            rest = after;
        }
        let mut depth = 0;
        while let Some((key, after)) = rest.split_first() {
            depth += 1;
            if depth > crate::limits::Limits::SEMANTIC_DEPTH {
                return Ok(None);
            }
            expected = match self.resolve(expected, node)? {
                Expected::Fields {
                    fields,
                    class,
                    union,
                } => match fields.iter().find(|field| field.name == *key) {
                    Some(field) => {
                        let Some((entry, value)) = self.entry(node, key) else {
                            return Ok(None);
                        };
                        located = Located {
                            mapping: node,
                            entry,
                            kept: schema.descriptors()[field.descriptor].descriptor.required(),
                        };
                        node = value;
                        rest = after;
                        Expected::Descriptor(field.descriptor)
                    }
                    None => {
                        // A value written through its shorthand is named
                        // through it: step into the field the shorthand fills
                        // and read the same key there.
                        let Some(filled) = class.and_then(|class| {
                            union.iter().find_map(|member| {
                                member
                                    .node_text(member.root())
                                    .and_then(|text| shorthand_field(schema, class, text))
                            })
                        }) else {
                            return Ok(None);
                        };
                        let Some((entry, value)) = self.entry(node, &filled.name) else {
                            return Ok(None);
                        };
                        located = Located {
                            mapping: node,
                            entry,
                            kept: true,
                        };
                        node = value;
                        Expected::Descriptor(filled.descriptor)
                    }
                },
                Expected::Registry(registry) => {
                    let N::Mapping(items) = &self.document.nodes()[node] else {
                        return Ok(None);
                    };
                    let &[(keyword, payload)] = items.as_slice() else {
                        return Ok(None);
                    };
                    if !matches!(&self.document.nodes()[keyword], N::Text(name) if name == key) {
                        return Ok(None);
                    }
                    let Some(entry) = schema.registries()[registry]
                        .entries
                        .iter()
                        .find(|entry| entry.name == *key)
                    else {
                        return Ok(None);
                    };
                    // The keyword is the value itself: clear its owning field.
                    located = Located {
                        mapping: node,
                        entry: 0,
                        kept: true,
                    };
                    node = payload;
                    rest = after;
                    match &entry.shape {
                        SchemaShape::Class(fields) => Expected::Fields {
                            fields,
                            class: None,
                            union: &[],
                        },
                        SchemaShape::Descriptor(descriptor) => Expected::Descriptor(*descriptor),
                    }
                }
                Expected::Dictionary { expression, value } => {
                    let Some((entry, child)) = self.entry(node, key) else {
                        return Ok(None);
                    };
                    located = Located {
                        mapping: node,
                        entry,
                        kept: false,
                    };
                    node = child;
                    rest = after;
                    Expected::Type {
                        expression,
                        node: value,
                        union: &[],
                    }
                }
                _ => return Ok(None),
            };
        }
        let N::Mapping(items) = &self.document.nodes()[located.mapping] else {
            return Ok(None);
        };
        let (_, value) = items[located.entry];
        if self.defaulted(value) {
            return Ok(None);
        }
        Ok(Some(located))
    }

    /// The concrete shape `expected` gives the value at `node`: a union is
    /// read as the first member the value matches, as composition reads it.
    fn resolve(
        &self,
        expected: Expected<'a>,
        node: usize,
    ) -> Result<Expected<'a>, NormalizationError> {
        let schema: &'a SchemaStructure = self.schema;
        let mut expected = expected;
        for _ in 0..=crate::limits::Limits::SEMANTIC_DEPTH {
            expected = match expected {
                Expected::Descriptor(descriptor) => {
                    let members = schema.descriptors()[descriptor].descriptor.members();
                    let mut chosen = None;
                    for member in members {
                        if matches_member(
                            schema,
                            self.document,
                            (member, member.root()),
                            node,
                            true,
                            NullPolicy::Value,
                            &[],
                        )? {
                            chosen = Some(member);
                            break;
                        }
                    }
                    let Some(member) = chosen else {
                        return Ok(Expected::Leaf);
                    };
                    Expected::Type {
                        expression: member,
                        node: member.root(),
                        union: members,
                    }
                }
                Expected::Type {
                    expression,
                    node: type_node,
                    union,
                } => match &expression.nodes()[type_node] {
                    TypeNode::List(_) => Expected::Leaf,
                    TypeNode::Dictionary { value, .. } => Expected::Dictionary {
                        expression,
                        value: *value,
                    },
                    TypeNode::Name(_) => {
                        let name = expression.node_text(type_node).unwrap_or_default();
                        if let Some(class) = schema.class_named(name) {
                            Expected::Fields {
                                fields: &class.fields,
                                class: Some(class),
                                union,
                            }
                        } else if let Some((_, alias)) = schema.alias_named(name) {
                            match alias.kind {
                                SchemaAliasKind::Descriptor(descriptor) => {
                                    Expected::Descriptor(descriptor)
                                }
                                SchemaAliasKind::Registry(registry) => Expected::Registry(registry),
                            }
                        } else {
                            Expected::Leaf
                        }
                    }
                },
                settled => return Ok(settled),
            };
        }
        Ok(Expected::Leaf)
    }
}

/// A copy of the accumulated value without the entries a layer clears, each
/// kept node with its origin.
struct Copy<'a> {
    document: &'a Document,
    origins: &'a [Option<CompositionOrigin>],
    removed: &'a BTreeSet<(usize, usize)>,
    nodes: Vec<N>,
    kept: Vec<Option<CompositionOrigin>>,
}

impl Copy<'_> {
    fn node(&mut self, node: usize, depth: usize) -> Result<usize, NormalizationError> {
        if depth > crate::limits::Limits::SEMANTIC_DEPTH {
            return Err(NormalizationError::Limit {
                resource: super::NormalizationResource::Depth,
                limit: crate::limits::Limits::SEMANTIC_DEPTH,
            });
        }
        let value = match &self.document.nodes()[node] {
            N::Sequence(items) => {
                let mut copied = Vec::with_capacity(items.len());
                for &item in items {
                    copied.push(self.node(item, depth + 1)?);
                }
                N::Sequence(copied)
            }
            N::Mapping(items) => {
                let mut copied = Vec::with_capacity(items.len());
                for (index, &(key, value)) in items.iter().enumerate() {
                    if self.removed.contains(&(node, index)) {
                        continue;
                    }
                    copied.push((self.node(key, depth + 1)?, self.node(value, depth + 1)?));
                }
                N::Mapping(copied)
            }
            value => value.clone(),
        };
        self.nodes.push(value);
        self.kept.push(self.origins[node]);
        Ok(self.nodes.len() - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_document::Tree as T;

    #[test]
    fn a_finding_path_names_a_value_written_bare_through_its_shorthand() {
        let document = T::map(vec![
            (
                "columns",
                T::list(vec![T::map(vec![
                    ("name", T::text("X")),
                    (
                        "derivation",
                        T::map(vec![(
                            "value",
                            T::map(vec![(
                                "mapping",
                                T::map(vec![("dict", T::map(vec![("NOT DONE", T::text("Y"))]))]),
                            )]),
                        )]),
                    ),
                ])]),
            ),
            (
                "rows",
                T::list(vec![T::map(vec![("id", T::text("week 2"))])]),
            ),
        ])
        .document();
        let value = document
            .nodes()
            .iter()
            .position(|node| matches!(node, N::Text(text) if text == "value"))
            .unwrap();
        let transparent = |key: usize| key == value;
        let written = |path: &str| written_path(&document, &transparent, path);
        assert_eq!(
            written("columns.X.derivation.value.mapping.dict['NOT DONE']"),
            "columns.X.derivation.mapping.dict['NOT DONE']"
        );
        // A step beyond the composed values is kept as written.
        assert_eq!(
            written("columns.X.derivation.value.mapping.source"),
            "columns.X.derivation.mapping.source"
        );
        assert_eq!(written("columns[0].derivation"), "columns[0].derivation");
        assert_eq!(written("rows['week 2'].filter"), "rows['week 2'].filter");
        assert_eq!(written("$"), "$");
        assert_eq!(written("metadata.scope"), "metadata.scope");
        // A path this reader does not split is returned unchanged.
        assert_eq!(written("columns['open"), "columns['open");
    }
}
