//! Authored ODM declarations select fixed source admission before study capture.
use super::*;
use alloc::collections::BTreeSet;

pub(super) fn inputs(d: &Document) -> Result<BTreeSet<&str>, PrepareError> {
    fn visit<'a>(
        d: &'a Document,
        node: usize,
        depth: usize,
        found: &mut BTreeSet<&'a str>,
    ) -> Result<(), PrepareError> {
        if depth > 64 {
            return Err(PrepareError::Limit("odm_expression_depth"));
        }
        match &d.nodes()[node] {
            N::Mapping(fields) => {
                if let Some(payload) = d.field(node, "odm") {
                    let item = if matches!(d.nodes()[payload], N::Text(_)) {
                        text(d, payload)?
                    } else {
                        text(d, field(d, payload, "item")?)?
                    };
                    if let Some((dataset, _)) = item.split_once('.') {
                        found.insert(dataset);
                    }
                } else {
                    for &(_, child) in fields {
                        visit(d, child, depth + 1, found)?;
                    }
                }
            }
            N::Sequence(children) => {
                for &child in children {
                    visit(d, child, depth + 1, found)?;
                }
            }
            _ => (),
        }
        Ok(())
    }
    let root = d.root();
    let mut found = BTreeSet::new();
    for &column in sequence(d, field(d, root, "columns")?)? {
        if let Some(derivation) = d
            .field(column, "derivation")
            .filter(|&node| !matches!(d.nodes()[node], N::Null))
        {
            visit(d, derivation, 0, &mut found)?;
        }
    }
    for collection in ["rows", "intermediates"] {
        if let Some(nodes) = d
            .field(root, collection)
            .filter(|&node| !matches!(d.nodes()[node], N::Null))
        {
            for &node in sequence(d, nodes)? {
                if let Some(derivations) = d
                    .field(node, "derivations")
                    .filter(|&node| !matches!(d.nodes()[node], N::Null))
                {
                    for &(_, derivation) in mapping(d, derivations)? {
                        visit(d, derivation, 0, &mut found)?;
                    }
                }
            }
        }
    }
    Ok(found)
}
