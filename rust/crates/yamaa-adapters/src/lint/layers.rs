//! The layers a linted specification composes on, which the two findings
//! that read more than the file they report, `partial_restatement` and
//! `restated_inherited_value`, compare it with: the `parent` chain it
//! inherits and the fragments its layers include, followed from the file
//! through the local file resolver, captured by the spelling that located
//! each, decoded with the shared YAML reader and admitted as composition
//! admits them (LAYER-1 to LAYER-4). Why a layer cannot be read does not
//! matter here: the chain is then not read at all, and validation reports
//! why.

use std::path::Path;

use yamaa_core::file_preparation::AliasCache;
use yamaa_core::inheritance::{self as graph, VersionedChain};
use yamaa_core::language::{FileType, LANGUAGE_VERSION};
use yamaa_core::resolver::{Name, Resolver};
use yamaa_core::schema::{Document as Values, SchemaStructure};

use crate::file_resources::Resources;

/// The port a chain is read through, which also reads what its layers name,
/// such as the producer a copy entry copies from (SPEC-9).
pub(super) type Port<'a> = AliasCache<'a, Resources>;

/// What `then` reads from the chain of the specification `file` holds, whose
/// own text decodes to `entry`, given the specification closure the chain
/// was admitted against and the port it was read through; none when the
/// chain cannot be followed or admitted.
pub(super) fn read<T>(
    file: &Path,
    entry: Values,
    then: impl FnOnce(&SchemaStructure, &VersionedChain, &mut Port<'_>) -> Option<T>,
) -> Option<T> {
    let (mut resources, name) = crate::file_configuration::resources(file.to_str()?).ok()?;
    let source = Resolver::inspect(
        &mut resources,
        Name::Entry {
            declaring: None,
            written: &name,
        },
    )
    .ok()?;
    let captured = crate::shipped_schema::capture_versioned(FileType::Spec).ok()?;
    let schema = captured.structure();
    let mut port = AliasCache::new(&mut resources);
    let chain = graph::traverse_versioned(
        schema,
        LANGUAGE_VERSION,
        source,
        entry,
        None,
        &mut port,
        &mut crate::decoder::Decoder::<std::convert::Infallible>::default(),
    )
    .ok()?
    .chain;
    then(schema, &chain, &mut port)
}
