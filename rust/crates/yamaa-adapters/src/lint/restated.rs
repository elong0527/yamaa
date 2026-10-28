//! `restated_inherited_value`: a value a layer writes exactly as it inherits
//! it (LAYER-5, LAYER-6, LAYER-9).
//!
//! A layer's root fields compose apart from one another, and inside a value
//! that merges with the one it inherits, a member of a keyed collection, a
//! class, a mapping, or a registry value that names the same keyword, each
//! field, entry or member composes apart too; every other value replaces
//! what it inherits whole. A value written where it composes apart, equal to
//! the value it inherits there, composes the same specification as the
//! layer without it: it reads as a change to a reviewer, and it stops
//! following the inherited value if that later changes. The outermost such
//! value is reported, a scalar, a list, a whole registry value, a mapping or
//! a member alike, with the layers it inherits the value from. The header
//! every layer declares (SCHEMA-2), the identity a member is matched by and
//! a column a copy entry declares are never reported, and a value the layer
//! lists in `clear` is no longer inherited, so it is never reported either
//! (LAYER-9). Either meaning may be the author's, so no fix is offered.
//!
//! The layer is read as preparation reads it: its chain followed as
//! [`super::layers`] reads it, and each layer normalized at the version it
//! declares, its copy entries replaced by the columns their producers'
//! output contracts declare and its relative paths respelled from the entry
//! (SPEC-9, LAYER-10), then composed by the engine's own composition. What
//! the layer inherits is the composition of the layers before it with the
//! layer's own `clear` applied, and whether a value merges is what the
//! composition of the whole chain does with it. A file whose chain cannot be
//! followed, admitted, expanded or composed is not read for this finding:
//! validation reports why.

use std::collections::BTreeSet;
use std::path::Path;

use yamaa_core::inheritance::VersionedChain;
use yamaa_core::resolver::Resolver;
use yamaa_core::schema::{
    ComposedLayers, Document as Values, DocumentNode as N, NormalizedDocument, SchemaOrigin,
    SchemaSource,
};

use super::canonical::{correspond, same};
use super::checks::Departure;
use super::document::{Document, Value};
use super::inheritance::prose;
use super::layers::Port;

/// The root fields every layer declares, its header (SCHEMA-2), which a
/// layer writes again without restating anything.
const HEADER: [&str; 2] = ["yamaa", "type"];

/// Every `restated_inherited_value` in `document`, the specification read
/// from `file`; none for a file that declares neither `parent` nor
/// `include`.
pub(super) fn check(document: &Document, file: &Path) -> Vec<Departure> {
    restatements(document, file).unwrap_or_default()
}

fn restatements(document: &Document, file: &Path) -> Option<Vec<Departure>> {
    let entry = &document.decoded.as_ref()?.document;
    let layered = ["parent", "include"]
        .iter()
        .any(|field| document.member(document.root, field).is_some());
    if !layered || !yamaa_core::language::declares_header(entry) {
        return None;
    }
    let nodes = correspond(document, entry)?;
    super::layers::read(file, entry.clone(), |schema, chain, port| {
        let layers = crate::document::contributed_layers(schema, chain, port)?;
        let view = schema.at_version(chain.version);
        let (own, before) = layers.split_last()?;
        let mut inherits = before.to_vec();
        inherits.push(clearing(own)?);
        let inherited = view.compose_versioned_layers(&inherits).ok()?;
        let composed = view.compose_versioned_layers(&layers).ok()?;
        let walk = Walk::new(own, before.len(), &inherited, &composed, entry);
        Some(departures(
            document,
            &nodes,
            Restated {
                own,
                inherited: &inherited,
                chain,
                found: walk.run(),
            },
            port,
        ))
    })
}

/// The layer that writes only the `clear` of `own`, the entry layer: the
/// layers before it compose with it to what the entry inherits, since its
/// `clear` applies before its own fields (LAYER-9).
fn clearing(own: &NormalizedDocument) -> Option<NormalizedDocument> {
    let values = &own.document;
    let N::Mapping(fields) = &values.nodes()[values.root()] else {
        return None;
    };
    let mut nodes = Vec::new();
    let mut origins = Vec::new();
    let mut push = |node: N, from: usize| {
        nodes.push(node);
        origins.push(own.origins[from]);
        nodes.len() - 1
    };
    let mut kept = Vec::new();
    if let Some(&(key, list)) = fields
        .iter()
        .find(|&&(key, _)| text(values, key) == Some("clear"))
    {
        let N::Sequence(paths) = &values.nodes()[list] else {
            return None;
        };
        let mut items = Vec::with_capacity(paths.len());
        for &path in paths {
            let N::Text(written) = &values.nodes()[path] else {
                return None;
            };
            items.push(push(N::Text(written.clone()), path));
        }
        let key = push(N::Text("clear".into()), key);
        kept.push((key, push(N::Sequence(items), list)));
    }
    let root = push(N::Mapping(kept), values.root());
    Some(NormalizedDocument {
        document: Values::new(nodes, root).ok()?,
        origins,
    })
}

/// The text of a node that holds text.
fn text(values: &Values, node: usize) -> Option<&str> {
    match &values.nodes()[node] {
        N::Text(text) => Some(text),
        _ => None,
    }
}

/// The field a member of a keyed list is matched by and its value: `name`
/// when the member writes it, else `id`, as the spec path notation names a
/// member (P-CLR-2).
fn identity(values: &Values, member: usize) -> Option<(&'static str, &str)> {
    let field = if values.field(member, "name").is_some() {
        "name"
    } else {
        "id"
    };
    Some((field, text(values, values.field(member, field)?)?))
}

/// The entry layer as it composes, read beside what it inherits.
struct Walk<'a> {
    own: &'a Values,
    inherited: &'a Values,
    /// The entry layer as written, decoded: a member of a keyed list that a
    /// copy entry declares was written there as the name it lists.
    written: &'a Values,
    /// Where each node of the entry layer was written.
    origins: &'a [SchemaOrigin],
    /// Whether the composition of the whole chain holds each node of the
    /// entry layer as written: a value that replaces what it inherits, or
    /// that inherits nothing, so nothing inside it merges.
    replaces: Vec<bool>,
    /// Each value of the entry layer that restates what it inherits, with
    /// the inherited value.
    found: Vec<(usize, usize)>,
}

impl<'a> Walk<'a> {
    /// The walk of `own`, the entry layer, numbered `entry` in contribution
    /// order, beside `inherited`, given `composed`, the whole chain composed,
    /// and `written`, the entry as decoded.
    fn new(
        own: &'a NormalizedDocument,
        entry: usize,
        inherited: &'a ComposedLayers,
        composed: &ComposedLayers,
        written: &'a Values,
    ) -> Self {
        let mut replaces = vec![false; own.document.nodes().len()];
        for origin in &composed.node_origins {
            if origin.layer == Some(entry) {
                if let Some(slot) = replaces.get_mut(origin.origin.node) {
                    *slot = true;
                }
            }
        }
        Self {
            own: &own.document,
            inherited: &inherited.document,
            written,
            origins: &own.origins,
            replaces,
            found: Vec::new(),
        }
    }

    /// The values the entry layer restates, outermost first, in written
    /// order. Root fields compose apart, so the root merges.
    fn run(mut self) -> Vec<(usize, usize)> {
        self.within(self.own.root(), self.inherited.root(), &HEADER);
        self.found
    }

    /// `node`, a value of the entry layer written where it composes apart,
    /// beside `inherited`, the value it inherits: it restates that value, or
    /// it merges with it and the values it holds are read in turn. `skip`
    /// names the fields inside it that never restate.
    fn value(&mut self, node: usize, inherited: usize, skip: &[&str]) {
        if same(self.own, node, self.inherited, inherited) {
            self.found.push((node, inherited));
        } else if !self.replaces[node] {
            self.within(node, inherited, skip);
        }
    }

    /// The values held by `node`, a value of the entry layer that merges
    /// with `inherited`: a mapping's entries by key, the fields in `skip`
    /// left out, and a keyed list's members by identity, each member's
    /// identity left out.
    fn within(&mut self, node: usize, inherited: usize, skip: &[&str]) {
        let (own, before) = (self.own, self.inherited);
        match (&own.nodes()[node], &before.nodes()[inherited]) {
            (N::Mapping(fields), N::Mapping(inherits)) => {
                for &(key, value) in fields {
                    if text(own, key).is_some_and(|name| skip.contains(&name)) {
                        continue;
                    }
                    if let Some(&(_, old)) = inherits
                        .iter()
                        .find(|&&(other, _)| same(own, key, before, other))
                    {
                        self.value(value, old, &[]);
                    }
                }
            }
            (N::Sequence(members), N::Sequence(inherits)) => {
                for &member in members {
                    let Some((field, id)) = identity(own, member) else {
                        continue;
                    };
                    if !self.declared(member, field) {
                        continue;
                    }
                    if let Some(&old) = inherits
                        .iter()
                        .find(|&&old| identity(before, old) == Some((field, id)))
                    {
                        self.value(member, old, &[field]);
                    }
                }
            }
            _ => {}
        }
    }

    /// Whether `member` of a keyed list was written in the entry layer as a
    /// member that writes `field`, not declared by a copy entry (SPEC-9).
    fn declared(&self, member: usize, field: &str) -> bool {
        let origin = self.origins[member];
        matches!(origin.source, SchemaSource::Input)
            && self.written.field(origin.node, field).is_some()
    }
}

/// What the walk found, with what it needs to name each finding.
struct Restated<'a> {
    own: &'a NormalizedDocument,
    inherited: &'a ComposedLayers,
    chain: &'a VersionedChain,
    found: Vec<(usize, usize)>,
}

/// The findings at the values the walk found, each at the key of the value
/// as written, or at the member a list writes it as, once per written node.
/// `nodes` gives the lint's node for each node of the file as decoded.
fn departures(
    document: &Document,
    nodes: &[usize],
    restated: Restated<'_>,
    port: &mut Port<'_>,
) -> Vec<Departure> {
    let mut keys = vec![None; document.nodes.len()];
    for node in &document.nodes {
        if let Value::Mapping { pairs, .. } = &node.value {
            for &(key, value) in pairs {
                keys[value] = Some(key);
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut found = Vec::new();
    for &(node, inherited) in &restated.found {
        let origin = restated.own.origins[node];
        if !matches!(origin.source, SchemaSource::Input) {
            continue;
        }
        let Some(&at) = nodes.get(origin.node).filter(|&&at| at != usize::MAX) else {
            continue;
        };
        if !seen.insert(at) {
            continue;
        }
        let Some(from) = sources(&restated, inherited, port) else {
            continue;
        };
        let start = document.node(keys[at].unwrap_or(at)).start;
        let path = document.path(at);
        found.push(Departure::new(
            "restated_inherited_value",
            start.line,
            start.column,
            path,
            format!(
                "`{path}` writes the value it inherits from {from}; remove it to keep inheriting"
            ),
        ));
    }
    found
}

/// The layers that wrote `inherited`, a value of what the entry inherits,
/// in contribution order, each spelled from the entry's directory as the
/// resolver respells a path (LAYER-10), and joined as prose.
fn sources(restated: &Restated<'_>, inherited: usize, port: &mut Port<'_>) -> Option<String> {
    let values = &restated.inherited.document;
    let mut layers = BTreeSet::new();
    let mut pending = vec![inherited];
    while let Some(node) = pending.pop() {
        layers.extend(restated.inherited.node_origins[node].layer);
        match &values.nodes()[node] {
            N::Sequence(items) => pending.extend(items),
            N::Mapping(pairs) => pending.extend(pairs.iter().map(|&(_, value)| value)),
            _ => {}
        }
    }
    let entry = &restated.chain.layers.last()?.source;
    let names = layers
        .into_iter()
        .filter_map(|layer| restated.chain.layers.get(layer))
        .filter(|layer| layer.source.identity != entry.identity)
        .map(|layer| {
            let source = &layer.source;
            let name = source.identity.rsplit('/').next().unwrap_or_default();
            port.rebase(source, entry, name)
                .unwrap_or_else(|_| source.display_path.clone())
        })
        .collect::<Vec<_>>();
    (!names.is_empty()).then(|| prose(&names))
}
