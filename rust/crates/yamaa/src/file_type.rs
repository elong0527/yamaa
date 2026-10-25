//! The type a yamaa file declares in its `type` header (SCHEMA-2): `spec`,
//! `project`, `codelists` or `function`. The command line reads it with
//! [`Engine::file_type`] to choose what a command runs (#3030 D122); the
//! hosts do not call it, since each of their calls names the type it reads.
//! [`crate::schema()`] takes a type, as written, and gives its JSON Schema.

use crate::engine::Engine;
use crate::error::Failure;
use crate::text::Action;

/// The file types as written, one for each [`FileType`] in declaration order:
/// the closed vocabulary of the `type` header and of the type
/// [`crate::schema()`] takes.
pub const FILE_TYPES: [&str; 4] = ["spec", "project", "codelists", "function"];

/// The type a yamaa file declares in `type` (SCHEMA-2).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileType {
    /// A specification, `type: spec`.
    Spec,
    /// A project file, `type: project`.
    Project,
    /// A codelist file, `type: codelists`.
    Codelists,
    /// A function file, `type: function`.
    Function,
}

impl FileType {
    /// The type as a file writes it: `spec`, `project`, `codelists` or
    /// `function`, one of [`FILE_TYPES`].
    pub fn as_str(self) -> &'static str {
        self.into_core().name()
    }

    /// The type a file writes as `text`, compared exactly; `None` for any
    /// other text.
    pub fn parse(text: &str) -> Option<Self> {
        yamaa_core::language::FileType::parse(text).map(Self::from_core)
    }

    fn from_core(file_type: yamaa_core::language::FileType) -> Self {
        use yamaa_core::language::FileType as Core;
        match file_type {
            Core::Spec => Self::Spec,
            Core::Project => Self::Project,
            Core::Codelists => Self::Codelists,
            Core::Function => Self::Function,
        }
    }

    pub(crate) fn into_core(self) -> yamaa_core::language::FileType {
        use yamaa_core::language::FileType as Core;
        match self {
            Self::Spec => Core::Spec,
            Self::Project => Core::Project,
            Self::Codelists => Core::Codelists,
            Self::Function => Core::Function,
        }
    }
}

impl Engine {
    /// The type the file at `path` declares, read from its `yamaa` and `type`
    /// header before anything else reads the file: what the command line
    /// dispatches on (#3030 D122).
    ///
    /// The file is read once through the engine's resolver, so a path that
    /// reaches no regular file, or reaches a link, is refused as any read
    /// refuses it, naming the path as given (FILE-7, FILE-8, FILE-11). Only a
    /// 1.x file declares a type: a file that declares `schema_version` is
    /// refused as `unsupported_language_version` at `schema_version`, naming
    /// language version 0.5. A 1.x file is refused as a call that reads its
    /// declared type refuses its header: a version the engine does not read
    /// as `unsupported_language_version`, and a `type` naming none of the
    /// four types as `unexpected_file_type` (SCHEMA-2). A refusal names the
    /// call as `cannot read <path>` (D39); the command line reports it as a
    /// file problem, with exit status 1 (D68).
    ///
    /// # Example
    ///
    /// An engine that claims no host language, as the command line's does,
    /// reads through a memory resolver that serves a 1.0 specification and a
    /// file that declares `schema_version`, which has no type, at its first
    /// line. A refusal locates its issue in the file by the reference the
    /// resolver gave it. Lanes without the `yaml` feature refuse both.
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use yamaa::{Engine, EngineOptions, FileType, MemoryResolver};
    ///
    /// let resolver = Arc::new(MemoryResolver::new());
    /// resolver.insert("/", "dm.yaml", "/study/dm.yaml", Arc::from(
    ///     &b"yamaa: \"1.0\"\ntype: spec\ndomain: DM\n"[..],
    /// ));
    /// resolver.insert("/", "ae.yaml", "/study/ae.yaml", Arc::from(
    ///     &b"schema_version: \"1.0\"\ndomain: AE\n"[..],
    /// ));
    /// let engine = Engine::new(
    ///     EngineOptions::neutral("doctest".into(), "0".into()).with_resolver(resolver),
    /// );
    /// let refused = engine.file_type("ae.yaml").unwrap_err();
    /// if cfg!(feature = "yaml") {
    ///     assert_eq!(engine.file_type("dm.yaml").unwrap(), FileType::Spec);
    ///     assert_eq!(
    ///         refused.message,
    ///         "cannot read ae.yaml: /study/ae.yaml:1:1: this engine does not read \
    ///          language version 0.5 [unsupported_language_version, SCHEMA-2]"
    ///     );
    /// } else {
    ///     assert!(engine.file_type("dm.yaml").is_err());
    /// }
    /// assert_eq!(FileType::parse("codelists"), Some(FileType::Codelists));
    /// assert_eq!(FileType::Codelists.as_str(), "codelists");
    /// ```
    pub fn file_type(&self, path: &str) -> Result<FileType, Failure> {
        yamaa_application::file_application::file_type_located(
            path,
            crate::internal::resolver_bridge(self.shared()),
        )
        .map(FileType::from_core)
        .map_err(|(issues, locator)| {
            Failure::rejected(
                Action::Read(path),
                &crate::diagnostic::Report::located(&issues, &locator),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_type_reads_as_a_file_writes_it() {
        for (file_type, text) in [
            (FileType::Spec, "spec"),
            (FileType::Project, "project"),
            (FileType::Codelists, "codelists"),
            (FileType::Function, "function"),
        ] {
            assert_eq!(file_type.as_str(), text);
            assert_eq!(FileType::parse(text), Some(file_type));
            assert_eq!(FileType::from_core(file_type.into_core()), file_type);
        }
        for text in ["", "Spec", "codelist", "environment", " spec"] {
            assert_eq!(FileType::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn file_types_are_the_types_in_order() {
        assert_eq!(
            [
                FileType::Spec,
                FileType::Project,
                FileType::Codelists,
                FileType::Function
            ]
            .map(FileType::as_str),
            FILE_TYPES
        );
        assert_eq!(
            yamaa_core::language::FileType::ALL.map(|kind| FileType::from_core(kind).as_str()),
            FILE_TYPES
        );
    }
}
