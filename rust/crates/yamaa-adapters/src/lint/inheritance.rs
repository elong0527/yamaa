//! `partial_restatement`: a layer that restates a mapping it inherits and
//! leaves out entries that then come back from its parent or from a fragment
//! it includes (LAYER-6, LAYER-8, LAYER-9).
//!
//! A layer that writes an inherited member merges with it: classes field by
//! field and mappings key by key, so an entry the layer leaves out keeps its
//! inherited value. Clearing the mapping in the same layer replaces it
//! instead. The finding reads the mappings whose entries a layer names: an
//! input's `types`, a named intermediate's `key` written as a mapping and its
//! `derivations`, a row template's `derivations`, the settings of a named
//! window, and the fields of the blocks `output` and `submission`, which
//! compose as a member does (LAYER-5). A layer restates one when it writes it
//! empty, or writes again an entry exactly as it inherits it; one that writes
//! only what it adds or changes is patching, which is never reported. A
//! block's required field that comes back is not named, since a layer writes
//! it rather than clears it (LAYER-9). Either meaning may be the author's, so
//! no fix is offered.
//!
//! Like `restated_inherited_value`, it reads more than the file it reports:
//! the layers the specification inherits and includes, read as
//! [`super::layers`] reads them, each normalized at the version it declares,
//! and composed by the engine's own composition in contribution order. A
//! file whose chain cannot be followed, admitted or composed is not read for
//! it: validation reports why.

use std::path::Path;

use yamaa_core::schema::{
    Document as Values, DocumentNode as N, NormalizedDocument, SchemaOrigin, SchemaStructure,
    BLOCKS,
};

use super::canonical::same;
use super::checks::Departure;
use super::document::Document;

/// The keyed collections whose members hold the mappings a layer restates;
/// with the blocks, the only root fields composed here (LAYER-5).
const COLLECTIONS: [&str; 4] = ["input", "intermediates", "rows", "windows"];

/// Whether the root field `name` is composed here.
fn composed_here(name: &str) -> bool {
    COLLECTIONS.contains(&name) || BLOCKS.contains(&name)
}

/// A mapping a layer may restate, named by the identities that reach it.
#[derive(Clone, Copy)]
enum Site<'a> {
    /// An input's `types`: `input.<input>.types`.
    Types { input: &'a str },
    /// A field of a named intermediate or a row template:
    /// `<collection>.<id>.<field>`.
    Member {
        collection: &'static str,
        id: &'a str,
        field: &'static str,
    },
    /// A named window, whose settings compose as a class's fields
    /// (LAYER-8): `windows.<name>`.
    Window { name: &'a str },
    /// A block, `output` or `submission`, whose fields compose as a class's
    /// (LAYER-5).
    Block { field: &'static str },
}

/// The fields of a keyed member whose value a layer may restate, by
/// collection: an intermediate's `key` composes key by key only when it is a
/// mapping (LAYER-6).
const MEMBER_FIELDS: [(&str, &[&str]); 2] = [
    ("intermediates", &["key", "derivations"]),
    ("rows", &["derivations"]),
];

/// Every `partial_restatement` in `document`, the specification read from
/// `file`; none for a file that declares neither `parent` nor `include`.
pub(super) fn check(document: &Document, file: &Path) -> Vec<Departure> {
    restatements(document, file).unwrap_or_default()
}

fn restatements(document: &Document, file: &Path) -> Option<Vec<Departure>> {
    let entry = &document.decoded.as_ref()?.document;
    // What the layer composes on, as its message names it.
    let from = match (
        document.member(document.root, "parent").is_some(),
        document.member(document.root, "include").is_some(),
    ) {
        (true, false) => "the parent",
        (false, true) => "an included fragment",
        (true, true) => "the parent or an included fragment",
        (false, false) => return None,
    };
    if !yamaa_core::language::declares_header(entry) {
        return None;
    }
    let entry = entry.clone();
    let (view, layers) = layers(file, entry)?;
    let (own, ancestors) = layers.split_last()?;
    let inherited = view.compose_versioned_layers(ancestors).ok()?.document;
    let composed = view.compose_versioned_layers(&layers).ok()?.document;
    let layer = &own.document;
    let mut found = Vec::new();
    for site in sites(layer) {
        let Some(mut back) = left_out(layer, &inherited, &composed, site) else {
            continue;
        };
        if let Site::Block { field } = site {
            let required = required(&view, field);
            back.retain(|key| !required.contains(key));
            if back.is_empty() {
                continue;
            }
        }
        let Some((key, value)) = written(document, site) else {
            continue;
        };
        let start = document.node(key).start;
        let path = document.path(value);
        found.push(Departure::new(
            "partial_restatement",
            start.line,
            start.column,
            path,
            message(site, path, &back, from),
        ));
    }
    Some(found)
}

/// The layers of the specification `file` holds, whose own text decodes to
/// `entry`: in contribution order, its fragments included, and the entry
/// last, each admitted and normalized at the version it declares and reduced
/// to the collections composed here, with the view of the version the chain
/// needs (P-VER-1).
fn layers(file: &Path, entry: Values) -> Option<(SchemaStructure, Vec<NormalizedDocument>)> {
    super::layers::read(file, entry, |schema, chain, _| {
        let layers = chain
            .layers
            .iter()
            .map(|layer| {
                let normalized = schema
                    .at_version(layer.version)
                    .normalize_versioned_layer(&layer.document)
                    .ok()?;
                reduced(&normalized)
            })
            .collect::<Option<Vec<_>>>()?;
        Some((schema.at_version(chain.version), layers))
    })
}

/// `layer` holding only the collections composed here and the `clear` paths
/// that may name them. Root fields compose apart from one another (LAYER-5),
/// so these compose as they do in the whole layer; the rest is left out, a
/// `copy` entry among it, which composes only once its producer is read
/// (SPEC-9).
fn reduced(layer: &NormalizedDocument) -> Option<NormalizedDocument> {
    let values = &layer.document;
    let N::Mapping(fields) = &values.nodes()[values.root()] else {
        return None;
    };
    let mut copied = Copied {
        layer,
        nodes: Vec::new(),
        origins: Vec::new(),
    };
    let mut kept = Vec::new();
    for &(key, value) in fields {
        let N::Text(name) = &values.nodes()[key] else {
            continue;
        };
        let value = if composed_here(name) {
            copied.copy(value)
        } else if name == "clear" {
            let N::Sequence(paths) = &values.nodes()[value] else {
                continue;
            };
            let paths = paths
                .iter()
                .filter(|&&path| text(values, path).is_none_or(names_collection))
                .map(|&path| copied.copy(path))
                .collect();
            copied.push(N::Sequence(paths), value)
        } else {
            continue;
        };
        kept.push((copied.copy(key), value));
    }
    let root = copied.push(N::Mapping(kept), values.root());
    Some(NormalizedDocument {
        document: Values::new(copied.nodes, root).ok()?,
        origins: copied.origins,
    })
}

/// Whether a `clear` path may name a value of a collection composed here. Its
/// first step is a root field, written as a bare segment (LAYER-9); a path
/// whose first step is written otherwise is kept, for composition to judge.
fn names_collection(path: &str) -> bool {
    let root = &path[..path.find(['.', '[']).unwrap_or(path.len())];
    let bare = !root.is_empty()
        && root
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    !bare || composed_here(root)
}

/// The required fields of the class the root field `block` holds, which a
/// layer writes to replace and never clears (LAYER-9).
fn required(view: &SchemaStructure, block: &str) -> Vec<String> {
    let class = view
        .root_class()
        .fields
        .iter()
        .find(|field| field.name == block)
        .and_then(
            |field| match view.descriptors()[field.descriptor].descriptor.members() {
                [member] => member.node_text(member.root()),
                _ => None,
            },
        )
        .and_then(|name| view.class_named(name));
    class.map_or_else(Vec::new, |class| {
        class
            .fields
            .iter()
            .filter(|field| view.descriptors()[field.descriptor].descriptor.required())
            .map(|field| field.name.clone())
            .collect()
    })
}

/// A rebuilt layer: each node copied keeps the origin of the node it copies.
struct Copied<'a> {
    layer: &'a NormalizedDocument,
    nodes: Vec<N>,
    origins: Vec<SchemaOrigin>,
}

impl Copied<'_> {
    fn push(&mut self, node: N, from: usize) -> usize {
        self.nodes.push(node);
        self.origins.push(self.layer.origins[from]);
        self.nodes.len() - 1
    }

    fn copy(&mut self, node: usize) -> usize {
        let layer = self.layer;
        let value = match &layer.document.nodes()[node] {
            N::Sequence(items) => N::Sequence(items.iter().map(|&item| self.copy(item)).collect()),
            N::Mapping(pairs) => N::Mapping(
                pairs
                    .iter()
                    .map(|&(key, value)| (self.copy(key), self.copy(value)))
                    .collect(),
            ),
            scalar => scalar.clone(),
        };
        self.push(value, node)
    }
}

/// The text of a node that holds text.
fn text(values: &Values, node: usize) -> Option<&str> {
    match &values.nodes()[node] {
        N::Text(text) => Some(text),
        _ => None,
    }
}

/// Every site `layer` writes that may restate a mapping, in written order.
fn sites(layer: &Values) -> Vec<Site<'_>> {
    let root = layer.root();
    let mut sites = Vec::new();
    if let Some(N::Mapping(inputs)) = layer.field(root, "input").map(|node| &layer.nodes()[node]) {
        for &(key, member) in inputs {
            if let (Some(input), Some(_)) = (text(layer, key), layer.field(member, "types")) {
                sites.push(Site::Types { input });
            }
        }
    }
    if let Some(N::Mapping(windows)) = layer
        .field(root, "windows")
        .map(|node| &layer.nodes()[node])
    {
        sites.extend(
            windows
                .iter()
                .filter_map(|&(key, _)| text(layer, key))
                .map(|name| Site::Window { name }),
        );
    }
    for field in BLOCKS {
        if matches!(
            layer.field(root, field).map(|node| &layer.nodes()[node]),
            Some(N::Mapping(_))
        ) {
            sites.push(Site::Block { field });
        }
    }
    for (collection, fields) in MEMBER_FIELDS {
        let Some(N::Sequence(members)) = layer
            .field(root, collection)
            .map(|node| &layer.nodes()[node])
        else {
            continue;
        };
        for &member in members {
            let Some(id) = layer.field(member, "id").and_then(|node| text(layer, node)) else {
                continue;
            };
            for &field in fields {
                if layer.field(member, field).is_some() {
                    sites.push(Site::Member {
                        collection,
                        id,
                        field,
                    });
                }
            }
        }
    }
    sites
}

/// The value `site` names in `values`, a layer or a composition.
fn locate(values: &Values, site: Site<'_>) -> Option<usize> {
    let root = values.root();
    match site {
        Site::Types { input } => {
            let member = values.field(values.field(root, "input")?, input)?;
            values.field(member, "types")
        }
        Site::Member {
            collection,
            id,
            field,
        } => {
            let N::Sequence(members) = &values.nodes()[values.field(root, collection)?] else {
                return None;
            };
            let member = members.iter().copied().find(|&member| {
                values
                    .field(member, "id")
                    .is_some_and(|node| text(values, node) == Some(id))
            })?;
            values.field(member, field)
        }
        Site::Window { name } => values.field(values.field(root, "windows")?, name),
        Site::Block { field } => values.field(root, field),
    }
}

/// The entries of the mapping at `node`.
fn entries(values: &Values, node: usize) -> Option<&[(usize, usize)]> {
    match &values.nodes()[node] {
        N::Mapping(pairs) => Some(pairs),
        _ => None,
    }
}

/// The inherited entries that come back at `site`, in the order the
/// composition keeps them, when `layer` restates the mapping there: it
/// writes the mapping empty or writes again an entry exactly as `inherited`
/// holds it, and `composed`, the chain with `layer` composed last, holds an
/// entry `layer` does not write. A layer that clears the mapping, or each
/// entry it leaves out, leaves none to come back.
fn left_out(
    layer: &Values,
    inherited: &Values,
    composed: &Values,
    site: Site<'_>,
) -> Option<Vec<String>> {
    let written = entries(layer, locate(layer, site)?)?;
    let before = locate(inherited, site)?;
    entries(inherited, before)?;
    let after = entries(composed, locate(composed, site)?)?;
    let writes = |key: &str| {
        written
            .iter()
            .any(|&(other, _)| text(layer, other) == Some(key))
    };
    let back = after
        .iter()
        .filter_map(|&(key, _)| text(composed, key))
        .filter(|key| !writes(key))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let restates = written.is_empty()
        || written.iter().any(|&(key, value)| {
            text(layer, key)
                .and_then(|key| inherited.field(before, key))
                .is_some_and(|old| same(layer, value, inherited, old))
        });
    (restates && !back.is_empty()).then_some(back)
}

/// The key and value of `site` in the file as written.
fn written(document: &Document, site: Site<'_>) -> Option<(usize, usize)> {
    let at = |mapping: Option<usize>, name: &str| {
        Some((
            document.key(mapping, name)?,
            document.member(mapping, name)?,
        ))
    };
    match site {
        Site::Types { input } => {
            let member = document.member(document.member(document.root, "input"), input);
            at(member, "types")
        }
        Site::Member {
            collection,
            id,
            field,
        } => {
            let members = document.items(document.member(document.root, collection)?)?;
            let member = members.iter().copied().find(|&member| {
                document
                    .member(Some(member), "id")
                    .and_then(|node| document.text_scalar(node))
                    == Some(id)
            });
            at(member, field)
        }
        Site::Window { name } => at(document.member(document.root, "windows"), name),
        Site::Block { field } => at(document.root, field),
    }
}

/// `items` joined as prose: `a`, `a and b`, `a, b and c`.
pub(super) fn prose(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The finding's message: the entries that come back, and the `clear` paths
/// that drop them. A named window is a keyed member, which `clear` never
/// names, so each left-out setting is cleared by its own path (LAYER-8,
/// LAYER-9), and so is each left-out field of a block, since `output` is
/// required and never cleared.
fn message(site: Site<'_>, path: &str, back: &[String], from: &str) -> String {
    let quoted = |text: &str| format!("`{text}`");
    let names = prose(&back.iter().map(|key| quoted(key)).collect::<Vec<_>>());
    let (verb, pronoun) = if back.len() == 1 {
        ("comes", "it")
    } else {
        ("come", "them")
    };
    let clear = match site {
        Site::Window { .. } | Site::Block { .. } => {
            let paths = back
                .iter()
                .map(|key| quoted(&format!("{path}.{key}")))
                .collect::<Vec<_>>();
            format!("list {} in `clear` to drop {pronoun}", prose(&paths))
        }
        Site::Types { .. } | Site::Member { .. } => {
            format!("list `{path}` in `clear` to replace the mapping")
        }
    };
    format!("{names} {verb} back from {from}; {clear}, or write only what this layer changes")
}
