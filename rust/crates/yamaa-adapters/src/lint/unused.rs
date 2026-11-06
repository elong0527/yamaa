//! `unused_declaration`: an input, a named intermediate, a column or a row
//! template's derivation of a column that a specification writes and that
//! the resolved specification prunes, since no root reaches it (LAYER-11).
//! Such a declaration cannot affect the artifact, and a reference inside it
//! is never checked (LAYER-12), so a broken declaration that nothing reads
//! passes validation.
//!
//! The specification is resolved as preparation resolves it: its chain
//! followed as [`super::layers`] reads it, then composed, pruned and
//! normalized by the engine's own preparation, which names each declaration
//! it prunes with the declarations that read it. A declaration is reported
//! in the file that writes it, the chain's entry: where the entry declares
//! it, or, for a member a parent or a fragment declares, where the entry
//! writes into it. What only a parent or a fragment writes is never reported
//! here, since a layer drops an inherited member by leaving it out of
//! `output.columns` (LAYER-9). The roots LAYER-11 names survive, and so
//! does an intermediate that only reaches its refusal (LOOKUP-3); an input
//! only such an intermediate reads is not reported either. A fragment
//! composes on whatever includes it, so it is not read for this finding, nor
//! is a specification that does not resolve: validation reports why. Either
//! remedy, deleting the declaration or reading it, may be the author's, so no
//! fix is offered.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use yamaa_core::inheritance::VersionedChain;
use yamaa_core::inheritance_preparation::PrunedVersioned;
use yamaa_core::language::FileType;
use yamaa_core::resolution::Declaration;
use yamaa_core::schema::{LayerNodeOrigin, SchemaSource};

use super::canonical::{correspond, header};
use super::checks::Departure;
use super::document::{Document, Value};
use super::inheritance::prose;
use super::layers::Port;

/// Every `unused_declaration` in `document`, the specification read from
/// `file`.
pub(super) fn check(document: &Document, file: &Path) -> Vec<Departure> {
    unused(document, file).unwrap_or_default()
}

fn unused(document: &Document, file: &Path) -> Option<Vec<Departure>> {
    if document.included {
        return None;
    }
    let entry = &document.decoded.as_ref()?.document;
    if header(entry)?.1 != FileType::Spec {
        return None;
    }
    let nodes = correspond(document, entry)?;
    super::layers::read(file, entry.clone(), |schema, chain, port| {
        let resolved = crate::document::resolved_chain(schema, chain, port)?;
        Some(departures(document, &nodes, chain, &resolved.pruned, port))
    })
}

/// The findings at the pruned declarations the entry of `chain` writes, each
/// at the key of the value as written, or at the first character of the
/// member a list writes. `nodes` gives the lint's node for each node of the
/// entry as decoded.
fn departures(
    document: &Document,
    nodes: &[usize],
    chain: &VersionedChain,
    pruned: &[PrunedVersioned],
    port: &mut Port<'_>,
) -> Vec<Departure> {
    let entry = chain.layers.len() - 1;
    let writes_here = |origin: &LayerNodeOrigin| origin.layer == Some(entry);
    let removed = pruned
        .iter()
        .map(|declared| &declared.declaration)
        .collect::<BTreeSet<_>>();
    let mut keys = vec![None; document.nodes.len()];
    for node in &document.nodes {
        if let Value::Mapping { pairs, .. } = &node.value {
            for &(key, value) in pairs {
                keys[value] = Some(key);
            }
        }
    }
    let mut written = BTreeMap::new();
    for (path, node) in document.walk() {
        written.entry(path).or_insert(node);
    }
    let mut found = Vec::new();
    for declared in pruned {
        if declared
            .readers
            .iter()
            .any(|reader| !removed.contains(reader))
        {
            continue;
        }
        let Some(((path, origin), inside)) = declared.writes.split_first() else {
            continue;
        };
        let fields = inside
            .iter()
            .filter(|(_, origin)| writes_here(origin))
            .map(|(field, _)| format!("`{field}`"))
            .collect::<Vec<_>>();
        if !writes_here(origin) && fields.is_empty() {
            continue;
        }
        // The member as the entry writes it, or else the name a copy entry
        // lists it by (SPEC-9).
        let at = written.get(path).copied().or_else(|| {
            let listed = writes_here(origin) && matches!(origin.origin.source, SchemaSource::Input);
            listed
                .then(|| nodes.get(origin.origin.node).copied())
                .flatten()
                .filter(|&at| at != usize::MAX)
        });
        let Some(at) = at else {
            continue;
        };
        let mut message = format!("`{path}` {}", reason(declared));
        if !writes_here(origin) {
            let from = origin
                .layer
                .and_then(|layer| super::layers::spelled(chain, layer, port))
                .unwrap_or_else(|| "a layer it inherits".to_owned());
            message.push_str(&format!(
                "; {from} declares it, and this layer writes {}",
                prose(&fields)
            ));
        }
        let start = document.node(keys[at].unwrap_or(at)).start;
        found.push(Departure::new(
            "unused_declaration",
            start.line,
            start.column,
            path.clone(),
            message,
        ));
    }
    found
}

/// Why the resolved specification prunes `declared`: nothing reads it, or
/// only declarations it prunes too; a column is also left out of
/// `output.columns`, and a row template's derivation derives a pruned
/// column.
fn reason(declared: &PrunedVersioned) -> String {
    if let Declaration::RowDerivation { column, .. } = &declared.declaration {
        let column = Declaration::Column(column.clone()).spec_path();
        return format!("derives `{column}`, which is pruned");
    }
    let readers = declared
        .readers
        .iter()
        .map(|reader| format!("`{}`", reader.spec_path()))
        .collect::<Vec<_>>();
    let read = match readers.as_slice() {
        [] => "nothing reads it".to_owned(),
        [_] => format!("only {} reads it, which is pruned", prose(&readers)),
        _ => format!("only {} read it, which are pruned", prose(&readers)),
    };
    match &declared.declaration {
        Declaration::Column(_) if readers.is_empty() => {
            format!("is pruned: it is not in `output.columns` and {read}")
        }
        Declaration::Column(_) => format!("is pruned: it is not in `output.columns`, and {read}"),
        _ => format!("is pruned: {read}"),
    }
}
