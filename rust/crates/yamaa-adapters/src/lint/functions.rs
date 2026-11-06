//! `uncalled_function`: a function file a project file lists in `functions`
//! that no specification of the project calls (FUNC-1, FUNC-4). Every listed
//! file is read and admitted on each check and run (FUNC-2), but only the
//! cases of a called function run (FUNC-12), so such a file goes stale
//! without a word.
//!
//! The project is read as preparation reads it: composed through its
//! `parent` chain (PROJECT-16) and its listed function files admitted, with
//! no host language, so no lock is read and nothing is activated. Each
//! specification a section lists (PROJECT-3, PROJECT-5) is resolved as
//! [`super::unused`] resolves one, and a function is called when a call to
//! it survives pruning in one of them (LAYER-11): a call in a pruned
//! declaration binds nothing. `functions` replaces whole across project
//! files, so only the file that writes it is read for this finding, and only
//! when the project it composes lists a specification. A project whose
//! listing, function files or specifications cannot be read is not read for
//! it: validation reports why. Either remedy, dropping the listing or adding
//! the call, may be the author's, so no fix is offered.

use std::collections::BTreeSet;
use std::path::Path;

use yamaa_core::ast::ExprKind;
use yamaa_core::language::FileType;

use super::canonical::header;
use super::checks::Departure;
use super::document::Document;
use super::inheritance::prose;

/// Every `uncalled_function` in `document`, the project file read from
/// `file`.
pub(super) fn check(document: &Document, file: &Path) -> Vec<Departure> {
    uncalled(document, file).unwrap_or_default()
}

fn uncalled(document: &Document, file: &Path) -> Option<Vec<Departure>> {
    let entry = &document.decoded.as_ref()?.document;
    if header(entry)?.1 != FileType::Project {
        return None;
    }
    let listed = document.items(document.member(document.root, "functions")?)?;
    let (mut resources, name) = crate::file_configuration::resources(file.to_str()?).ok()?;
    let closures = crate::document::Closures::shipped().ok()?;
    let prepared = yamaa_core::build::prepare_environment(
        &mut resources,
        &mut crate::decoder::Decoder::<std::convert::Infallible>::with_closures(closures),
        &name,
        None,
    )
    .ok()?;
    let sections = prepared.project.submission.as_ref().ok()?;
    let specifications = sections
        .sections
        .iter()
        .flat_map(|section| &section.specs)
        .collect::<Vec<_>>();
    if specifications.is_empty() {
        return None;
    }
    // Every path a project writes resolves from the project file (D8).
    let directory = file.parent()?;
    let mut called = BTreeSet::new();
    for written in specifications {
        called.extend(calls(&directory.join(&written.text))?);
    }
    let origins = &prepared.origins;
    let mut found = Vec::new();
    for (index, &item) in listed.iter().enumerate() {
        let declared = origins
            .functions
            .iter()
            .zip(&origins.function_files)
            .filter(|&(_, &file)| file == index)
            .map(|(function, _)| function)
            .collect::<Vec<_>>();
        if declared.is_empty() || declared.iter().any(|&name| called.contains(name)) {
            continue;
        }
        let names = declared
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>();
        let path = document.path(item);
        let written = document.text_scalar(item).unwrap_or_default();
        let start = document.node(item).start;
        found.push(Departure::new(
            "uncalled_function",
            start.line,
            start.column,
            path,
            format!(
                "`{path}` lists {} ({written}), which no specification of the project calls",
                prose(&names)
            ),
        ));
    }
    Some(found)
}

/// The project functions the specification at `path` calls once resolved;
/// none when it cannot be read or resolved.
fn calls(path: &Path) -> Option<BTreeSet<String>> {
    let bytes = std::fs::read(path).ok()?;
    let entry = crate::yaml_decode::decode_yaml(&bytes).ok()?.document;
    super::layers::read(path, entry, |schema, chain, port| {
        let resolved = crate::document::resolved_chain(schema, chain, port)?;
        let lowered = yamaa_core::front_end::v1::spec::lower(&resolved.document, None);
        let spec = lowered.spec();
        let mut names = BTreeSet::new();
        for site in spec.sites() {
            for call in spec.calls(site.root) {
                if let ExprKind::Call { function, .. } = &spec.expr(call).kind {
                    names.insert(function.name.clone());
                }
            }
        }
        Some(names)
    })
}
