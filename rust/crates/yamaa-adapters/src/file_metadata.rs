//! One descriptor-backed reader for build metadata and original document aliases.
//! Parsing, semantic admission and route selection remain in the engine.
use crate::{
    file_resources::{Error as ResourceError, Resources},
    project_source::{Kind, ProjectCapturePort, Reply, Request},
    specification_source::Source,
};
use yamaa_core::inheritance::{Source as Identity, SourceError};

pub(crate) struct Reader<'a> {
    pub(crate) resources: &'a mut Resources,
}
impl Reader<'_> {
    fn resource_identity(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Identity, ResourceError> {
        let identity = self.resources.resolve_from(declaring, written)?;
        let display_path = if is_absolute_spelling(written) {
            written.to_owned()
        } else {
            format!("{}/{written}", directory(declaring)?)
        };
        Ok(Identity {
            identity,
            display_path,
        })
    }
}
fn available(error: ResourceError) -> SourceError<ResourceError> {
    match error {
        ResourceError::Missing | ResourceError::Unreadable | ResourceError::NotRegularFile => {
            SourceError::Unavailable
        }
        error => SourceError::Raised(error),
    }
}
impl yamaa_core::file_preparation::NativePort for Reader<'_> {
    type Error = ResourceError;
    fn resolve_entry(
        &mut self,
        declaring: Option<&str>,
        written: &str,
    ) -> Result<String, ResourceError> {
        match declaring {
            Some(declaring) => self.resources.resolve_from(declaring, written),
            None => self.resources.resolve(written),
        }
    }
    fn capture_entry(
        &mut self,
        declaring: Option<&str>,
        written: &str,
    ) -> Result<Vec<u8>, ResourceError> {
        let (bytes, _) = match declaring {
            Some(declaring) => self.resources.capture_metadata_from(declaring, written),
            None => self.resources.capture_metadata(written),
        }?;
        Ok(bytes.to_vec())
    }
    /// A `parent` path. Its written form is judged before any resolver is
    /// asked for it (FILE-4, FILE-5, LAYER-2), as schema admission judges
    /// every other declared path: a backslash, an empty segment or a trailing
    /// separator, which a network share's `//server/share` root also writes,
    /// is refused as written whichever resolver serves the layers.
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Identity, SourceError<ResourceError>> {
        if let Some(failure) = yamaa_core::resource::written_path_failure(written) {
            return Err(SourceError::Raised(ResourceError::WrittenForm(failure)));
        }
        self.resource_identity(declaring, written)
            .map_err(available)
    }
    fn canonicalize_resource(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Identity, SourceError<ResourceError>> {
        self.resource_identity(declaring, written)
            .map_err(SourceError::Raised)
    }
    fn capture_resource(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Vec<u8>, SourceError<ResourceError>> {
        self.resources
            .capture_metadata_from(declaring, written)
            .map(|(bytes, _)| bytes.to_vec())
            .map_err(SourceError::Raised)
    }
    fn resource_failure(
        &self,
        error: &ResourceError,
    ) -> Option<yamaa_core::resource::ResourceFailure> {
        use yamaa_core::resource::ResourceFailure;
        match error {
            ResourceError::Missing => Some(ResourceFailure::Missing),
            ResourceError::NotRegularFile => Some(ResourceFailure::NotRegularFile),
            ResourceError::WrittenForm(failure) => Some(*failure),
            ResourceError::Changed => Some(ResourceFailure::ContentChanged),
            _ => None,
        }
    }

    fn capture_parent(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Vec<u8>, SourceError<ResourceError>> {
        self.resources
            .capture_metadata_from(declaring, written)
            .map(|(bytes, _)| bytes.to_vec())
            .map_err(available)
    }
    fn rebase(
        &mut self,
        layer: &Identity,
        entry: &Identity,
        written: &str,
    ) -> Result<String, ResourceError> {
        rebase(&layer.identity, &entry.identity, written).map_err(|unwritten| match unwritten {
            Unwritten::Unrooted => ResourceError::InvalidPath,
            Unwritten::Share { .. } => ResourceError::OutsideRoots,
        })
    }
    /// The rebase is lexical, so its refusal is derived again, with both
    /// roots, from the same identities.
    fn rebase_failure(
        &self,
        _error: &ResourceError,
        layer: &Identity,
        entry: &Identity,
        written: &str,
    ) -> Option<yamaa_core::diagnostic::Diagnostic> {
        share_refusal(layer, entry, written)
    }
    fn accept_alias(&mut self, declaring: &str, written: &str) -> Result<(), ResourceError> {
        self.resources
            .capture_metadata_from(declaring, written)
            .map(|_| ())
    }
    fn select_entry_base(&mut self, identity: &str) -> Result<(), ResourceError> {
        self.resources.select_entry_base(identity)
    }
    fn missing_alias() -> ResourceError {
        ResourceError::InvalidPath
    }
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
fn share_refusal(
    layer: &Identity,
    entry: &Identity,
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

impl ProjectCapturePort for Reader<'_> {
    type Error = ResourceError;
    fn capture(&mut self, request: Request<'_>) -> Result<Reply, Self::Error> {
        let identity = self
            .resources
            .resolve_from(request.declaring_source, request.written)?;
        let (bytes, _) = self
            .resources
            .capture_metadata_from(request.declaring_source, request.written)?;
        let source = Source {
            identity,
            bytes: bytes.to_vec(),
        };
        Ok(match request.kind {
            Kind::Lock => Reply::Lock {
                kind: crate::project_lock::kind(&source.bytes)
                    .map_err(|_| ResourceError::InvalidLock)?,
                source,
            },
            Kind::Environment | Kind::Function | Kind::Codelist => Reply::Document(source),
        })
    }
}

impl yamaa_core::build::MetadataAdmissionPort for Reader<'_> {
    type Error = ResourceError;

    fn verify_metadata(&mut self) -> Result<(), (String, Self::Error)> {
        self.resources.verify_captured()
    }
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
        let source = |identity: &str| Identity {
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
