//! The lexical path rules of the resolver port over one scope: the path a
//! finding shows for a metadata file, and the respelling of a layer's paths
//! from the entry (LAYER-10). Nothing here inspects or opens a file.
use crate::file_resources::Error as ResourceError;
use yamaa_core::resolver::Located;

/// The path a finding shows for the file `written` names from the file
/// `declaring`: the written path when it is rooted, else the declaring
/// file's directory joined with it. Lexical: a declaring identity that names
/// no rooted file has none.
pub(crate) fn display_path(declaring: &str, written: &str) -> Result<String, ResourceError> {
    Ok(if is_absolute_spelling(written) {
        written.to_owned()
    } else {
        format!("{}/{written}", directory(declaring)?)
    })
}
/// [`rebase`], refused as the resolver port refuses it: an identity that
/// names no rooted file is an invalid path, and a location on a network share
/// the entry is not on is outside the roots it reaches.
pub(crate) fn rebased(layer: &str, entry: &str, written: &str) -> Result<String, ResourceError> {
    rebase(layer, entry, written).map_err(|unwritten| match unwritten {
        Unwritten::Unrooted => ResourceError::InvalidPath,
        Unwritten::Share { .. } => ResourceError::OutsideRoots,
    })
}
fn directory(identity: &str) -> Result<&str, ResourceError> {
    identity
        .rsplit_once('/')
        .map(|(directory, _)| directory)
        .filter(|_| is_absolute_spelling(identity))
        .ok_or(ResourceError::InvalidPath)
}
/// Lexical root test for declaring identities and authored spellings:
/// `/`-rooted or drive-rooted (`X:/...`). These spellings are judged by
/// the resource grammar, never by host path semantics: `Path::is_absolute`
/// is host-dependent (on Windows a prefix-less root such as `/spec.yaml`
/// is not absolute), so it rejects canonical declaring identities there
/// and every parent, dictionary and cross-layer rebase fails as
/// `InvalidPath` (see #2769). Mirrors the admission grammar in
/// `yamaa_core::resource::written_path_failure`.
fn is_absolute_spelling(path: &str) -> bool {
    path.starts_with('/')
        || (path.len() >= 3
            && path.as_bytes()[0].is_ascii_alphabetic()
            && path.as_bytes()[1] == b':'
            && path.as_bytes()[2] == b'/')
}
/// Why [`rebase`] writes no path.
#[derive(Debug, PartialEq, Eq)]
enum Unwritten {
    /// An identity that names no rooted file.
    Unrooted,
    /// A location beside a layer on a network share the entry is not on: the
    /// layer's root, that share, and the entry's root (FILE-4).
    Share { layer: String, entry: String },
}
/// Respell the path `written` that the layer `layer` declares so that it names
/// the same location from the entry `entry` (LAYER-10). Lexical only:
/// preparation never inspects or opens a study/output path.
///
/// Each identity's directory is one root and its segments: `/`, a drive
/// (`C:`) or a network share (`//server/share`, D15) is one root. On the
/// entry's root, the path is respelled relative to the entry's directory, a
/// parent step at the root staying there (FILE-6). No relative path leaves
/// the entry's root, so a location on another root is written whole, as
/// LAYER-10 writes a location the entry cannot reach relatively: below `/` or
/// a drive, its absolute path, which file resolution walks from that root.
/// A location on another network share fails closed with both roots: no
/// written path names a share (FILE-4), and a parent step through the entry's
/// root would walk a local folder of the same name instead.
fn rebase(layer: &str, entry: &str, written: &str) -> Result<String, Unwritten> {
    let layer = rebase_directory(layer)?;
    let entry = rebase_directory(entry)?;
    if is_absolute_spelling(written) || same_directory(&layer, &entry) {
        return Ok(written.into());
    }
    let mut target = layer;
    for part in written.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                if target.len() > 1 {
                    target.pop();
                }
            }
            part => target.push(part.to_owned()),
        }
    }
    if !same_root(&target[0], &entry[0]) {
        if target[0].starts_with("//") {
            return Err(Unwritten::Share {
                layer: target[0].clone(),
                entry: root_name(&entry[0]),
            });
        }
        return Ok(format!("{}/{}", target[0], target[1..].join("/")));
    }
    let common = 1 + target[1..]
        .iter()
        .zip(&entry[1..])
        .take_while(|(target, entry)| crate::file_resources::same_location(target, entry))
        .count();
    let result = std::iter::repeat_n("..", entry.len() - common)
        .chain(target[common..].iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join("/");
    Ok(if result.is_empty() {
        ".".into()
    } else {
        result
    })
}
/// The root and segments of the directory that holds the file a declaring
/// identity names. A network share is one root on every host: rebasing is
/// lexical, and only a Windows walk gives an identity a share root.
fn rebase_directory(identity: &str) -> Result<Vec<String>, Unwritten> {
    let mut segments =
        crate::file_resolver::split_rooted(identity, true).ok_or(Unwritten::Unrooted)?;
    if segments.len() > 1 {
        segments.pop();
    }
    if segments[1..]
        .iter()
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(Unwritten::Unrooted);
    }
    Ok(segments)
}
/// A root as an identity spells it: `/` for the marker of `/`, otherwise the
/// drive (`C:`) or the share (`//server/share`).
fn root_name(root: &str) -> String {
    if root.is_empty() {
        "/".into()
    } else {
        root.into()
    }
}
/// The finding for a layer whose relative path `written` names a location on
/// a network share the entry is not on (LAYER-10): it names the layer, the
/// path as the layer wrote it, and the roots of the layer and of the entry.
/// Every other rebase, written or refused, has none.
pub(crate) fn share_refusal(
    layer: &Located,
    entry: &Located,
    written: &str,
) -> Option<yamaa_core::diagnostic::Diagnostic> {
    let Err(Unwritten::Share {
        layer: parent_root,
        entry: entry_root,
    }) = rebase(&layer.identity, &entry.identity, written)
    else {
        return None;
    };
    yamaa_core::diagnostic::InheritanceFinding::ParentOnAnotherShare {
        source: &layer.identity,
        path: written,
        parent_root: &parent_root,
        entry_root: &entry_root,
    }
    .diagnostic()
    .ok()
}
/// Whether two roots are one. Windows names drives and shares
/// case-insensitively; a host without them still compares a lexical drive or
/// share root that way, and `/` equals only itself.
fn same_root(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right) || crate::file_resources::same_location(left, right)
}
fn same_directory(left: &[String], right: &[String]) -> bool {
    left.len() == right.len()
        && same_root(&left[0], &right[0])
        && left[1..]
            .iter()
            .zip(&right[1..])
            .all(|(left, right)| crate::file_resources::same_location(left, right))
}

#[cfg(test)]
mod spelling_tests {
    use super::*;

    #[test]
    fn absolute_spelling_covers_both_root_forms_lexically() {
        assert!(is_absolute_spelling("/spec.yaml"));
        assert!(is_absolute_spelling("/layers/parent.yaml"));
        assert!(is_absolute_spelling("C:/study/spec.yaml"));
        assert!(!is_absolute_spelling("spec.yaml"));
        assert!(!is_absolute_spelling("layers/parent.yaml"));
        assert!(!is_absolute_spelling("../input/dm.csv"));
        assert!(!is_absolute_spelling("C:study/spec.yaml"));
        assert!(!is_absolute_spelling(""));
    }

    #[test]
    fn declaring_directory_accepts_canonical_identities_on_every_host() {
        // Canonical declaring identities are `/`-rooted on every host.
        // Judging them with `Path::is_absolute` rejects them on Windows,
        // where a prefix-less root is not absolute, so every parent,
        // dictionary and cross-layer rebase failed there as `InvalidPath`
        // (see #2769).
        assert_eq!(directory("/spec.yaml").unwrap(), "");
        assert_eq!(directory("/layers/parent.yaml").unwrap(), "/layers");
        assert_eq!(directory("C:/study/spec.yaml").unwrap(), "C:/study");
        assert!(directory("relative/spec.yaml").is_err());
        assert!(directory("spec.yaml").is_err());
    }

    #[test]
    fn rebase_reaches_parent_relative_paths_from_canonical_layers() {
        // The schema-inheritance-project-root layer declares its input as
        // `../input/dm.csv` beside `common/`; the entry rebases it to its
        // own directory. Same-directory layers keep their spelling.
        assert_eq!(
            rebase(
                "/common/spec_common.yaml",
                "/spec_study.yaml",
                "../input/dm.csv"
            )
            .unwrap(),
            "input/dm.csv"
        );
        assert_eq!(
            rebase(
                "/spec_organization.yaml",
                "/spec_study.yaml",
                "input/lb.csv"
            )
            .unwrap(),
            "input/lb.csv"
        );
    }

    #[test]
    fn rebase_respells_layers_on_the_entry_root_relative_to_the_entry() {
        for (layer, entry, written, rebased) in [
            (
                "/common/spec.yaml",
                "/study/spec.yaml",
                "dm.csv",
                "../common/dm.csv",
            ),
            ("/a/b/spec.yaml", "/a/spec.yaml", "../c/dm.csv", "c/dm.csv"),
            // A parent step at the root stays there (FILE-6).
            (
                "/common/spec.yaml",
                "/study/spec.yaml",
                "../../../dm.csv",
                "../dm.csv",
            ),
            (
                "C:/common/spec.yaml",
                "C:/study/spec.yaml",
                "dm.csv",
                "../common/dm.csv",
            ),
            (
                "C:/common/spec.yaml",
                "C:/study/spec.yaml",
                "../../dm.csv",
                "../dm.csv",
            ),
            // A drive letter is one root, whatever its case.
            (
                "c:/common/spec.yaml",
                "C:/study/spec.yaml",
                "dm.csv",
                "../common/dm.csv",
            ),
            // A network share is one root: its server and share are never
            // folders a parent step climbs through.
            (
                "//server/share/common/spec.yaml",
                "//server/share/study/spec.yaml",
                "dm.csv",
                "../common/dm.csv",
            ),
            (
                "//server/share/common/spec.yaml",
                "//server/share/study/spec.yaml",
                "../../../dm.csv",
                "../dm.csv",
            ),
            (
                "//server/share/spec.yaml",
                "//server/share/study/deep/spec.yaml",
                "input/dm.csv",
                "../../input/dm.csv",
            ),
            // One share, spelled in two cases, is one root.
            (
                "//SERVER/Share/common/spec.yaml",
                "//server/share/study/spec.yaml",
                "dm.csv",
                "../common/dm.csv",
            ),
            (
                "//SERVER/Share/study/spec.yaml",
                "//server/share/study/spec.yaml",
                "dm.csv",
                "dm.csv",
            ),
        ] {
            assert_eq!(
                rebase(layer, entry, written).unwrap(),
                rebased,
                "{layer} {entry} {written}"
            );
        }
    }

    #[test]
    fn rebase_writes_a_location_on_another_root_whole_and_never_climbs_to_it() {
        // Below `/` or a drive, the location is written as its absolute path
        // (LAYER-10), which file resolution walks from that root.
        for (layer, entry, written, rebased) in [
            (
                "D:/std/spec.yaml",
                "C:/study/spec.yaml",
                "input/dm.csv",
                "D:/std/input/dm.csv",
            ),
            (
                "D:/std/spec.yaml",
                "C:/study/spec.yaml",
                "../../../dm.csv",
                "D:/dm.csv",
            ),
            (
                "C:/std/spec.yaml",
                "//server/share/study/spec.yaml",
                "input/dm.csv",
                "C:/std/input/dm.csv",
            ),
            (
                "/std/spec.yaml",
                "C:/study/spec.yaml",
                "input/dm.csv",
                "/std/input/dm.csv",
            ),
            // A rooted path names its own location from any layer.
            (
                "//server/share/std/spec.yaml",
                "C:/study/spec.yaml",
                "D:/data/dm.csv",
                "D:/data/dm.csv",
            ),
        ] {
            assert_eq!(
                rebase(layer, entry, written).unwrap(),
                rebased,
                "{layer} {entry} {written}"
            );
        }
        // A location on a share is never respelled through the entry's root,
        // where `../../server/share/...` would walk local folders of the same
        // names, nor through another share: it fails closed with both roots,
        // since no written path names a share (FILE-4).
        for (layer, entry, layer_root, entry_root) in [
            (
                "//server/share/std/spec.yaml",
                "C:/study/spec.yaml",
                "//server/share",
                "C:",
            ),
            (
                "//server/share/std/spec.yaml",
                "/study/spec.yaml",
                "//server/share",
                "/",
            ),
            (
                "//other/share/std/spec.yaml",
                "//server/share/study/spec.yaml",
                "//other/share",
                "//server/share",
            ),
            (
                "//server/other/std/spec.yaml",
                "//server/share/study/spec.yaml",
                "//server/other",
                "//server/share",
            ),
        ] {
            for written in ["input/dm.csv", "../dm.csv", "./dm.csv"] {
                assert_eq!(
                    rebase(layer, entry, written),
                    Err(Unwritten::Share {
                        layer: layer_root.into(),
                        entry: entry_root.into(),
                    }),
                    "{layer} {entry} {written}"
                );
            }
        }
        // Identities that name no rooted file are refused.
        for (layer, entry) in [
            ("relative/spec.yaml", "/study/spec.yaml"),
            ("/study/spec.yaml", "//server"),
            ("/a//spec.yaml", "/study/spec.yaml"),
            ("/a/../spec.yaml", "/study/spec.yaml"),
        ] {
            assert_eq!(
                rebase(layer, entry, "dm.csv"),
                Err(Unwritten::Unrooted),
                "{layer} {entry}"
            );
        }
    }

    #[test]
    fn a_layer_on_another_share_is_refused_with_its_path_and_both_roots() {
        use yamaa_core::{diagnostic::ConditionCode, json::Value};
        let source = |identity: &str| Located {
            identity: identity.into(),
            display_path: identity.into(),
        };
        for (layer, entry, layer_root, entry_root) in [
            (
                "//server/share/std/spec.yaml",
                "C:/study/spec.yaml",
                "//server/share",
                "C:",
            ),
            (
                "//server/share/std/spec.yaml",
                "/study/spec.yaml",
                "//server/share",
                "/",
            ),
            (
                "//server/share/std/spec.yaml",
                "//other/share/study/spec.yaml",
                "//server/share",
                "//other/share",
            ),
        ] {
            let finding = share_refusal(&source(layer), &source(entry), "input/dm.csv")
                .expect("a layer on another share is refused with a finding");
            assert_eq!(finding.code, ConditionCode::InheritanceParentOnAnotherShare);
            assert_eq!(finding.spec_paths, ["parent"]);
            assert_eq!(
                finding.context,
                [
                    ("entry_root".into(), Value::String(entry_root.into())),
                    ("parent_root".into(), Value::String(layer_root.into())),
                    ("path".into(), Value::String("input/dm.csv".into())),
                    ("source".into(), Value::String(layer.into())),
                ]
                .into(),
                "{layer} {entry}"
            );
        }
        // A location the entry writes, relatively or whole, and an identity
        // that names no rooted file have no such finding.
        for (layer, entry, written) in [
            ("D:/std/spec.yaml", "C:/study/spec.yaml", "input/dm.csv"),
            (
                "C:/std/spec.yaml",
                "//server/share/study/spec.yaml",
                "input/dm.csv",
            ),
            (
                "//server/share/std/spec.yaml",
                "//SERVER/Share/study/spec.yaml",
                "input/dm.csv",
            ),
            (
                "//server/share/std/spec.yaml",
                "C:/study/spec.yaml",
                "D:/data/dm.csv",
            ),
            ("relative/spec.yaml", "C:/study/spec.yaml", "input/dm.csv"),
        ] {
            assert_eq!(
                share_refusal(&source(layer), &source(entry), written),
                None,
                "{layer} {entry} {written}"
            );
        }
    }
}
