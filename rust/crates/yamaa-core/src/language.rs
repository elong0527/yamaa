//! The header a yamaa document's root declares: the language version the
//! document needs and its file type (SCHEMA-2, LAYER-4, P-VER-1).
//!
//! A root that declares `schema_version`, or no `yamaa`, is read by the 0.5
//! language at the `yamaa-v0.5.0` tag (#3030 D75). A root that declares `yamaa`
//! is a 1.x document: its header is read here, before any schema closure is
//! chosen, and the declared type then selects the 1.0 closure. The engine
//! reads language version 1.0 ([`LANGUAGE_VERSION`], #3030 D76): every public
//! call routes a file it reads by the header the file declares.
use crate::diagnostic::{ConditionCode, Context, Diagnostic};
use crate::json::Value as JsonValue;
use crate::schema::{Document, DocumentNode as N, SchemaContext as C, SchemaDiagnostic};
use std::fmt;

/// A yamaa language version, written `"1.N"`; only major version 1 exists.
/// Versions order by their minor number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LanguageVersion {
    minor: u32,
}

impl LanguageVersion {
    /// Version `1.minor`.
    pub const fn new(minor: u32) -> Self {
        Self { minor }
    }
    /// Read a version written `1.N`, with `N` a decimal number without a
    /// leading zero. Any other text names no version.
    pub fn parse(text: &str) -> Option<Self> {
        let minor = text.strip_prefix("1.")?;
        let canonical =
            minor == "0" || (minor.bytes().all(|b| b.is_ascii_digit()) && !minor.starts_with('0'));
        if minor.is_empty() || !canonical {
            return None;
        }
        minor.parse().ok().map(Self::new)
    }
    pub fn minor(self) -> u32 {
        self.minor
    }
    /// Every version an engine of this version reads, oldest first: a 1.M
    /// engine reads 1.0 to 1.M (SCHEMA-2).
    pub fn readable(self) -> impl Iterator<Item = LanguageVersion> {
        (0..=self.minor).map(Self::new)
    }
}

impl fmt::Display for LanguageVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "1.{}", self.minor)
    }
}

/// The newest language version this engine implements. A 1.M engine reads
/// every 1.x file up to 1.M (SCHEMA-2); `info()` lists those versions, oldest
/// first, as `language_versions` (P-INFO-1).
pub const LANGUAGE_VERSION: LanguageVersion = LanguageVersion::new(0);

/// The versions an engine whose newest is `reads` reads, oldest first; none
/// for an engine that reads no 1.x version.
pub fn language_versions(reads: Option<LanguageVersion>) -> Vec<String> {
    reads
        .into_iter()
        .flat_map(LanguageVersion::readable)
        .map(|version| version.to_string())
        .collect()
}

/// The type a yamaa document declares in `type` (D4, P-TYPE-1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileType {
    Spec,
    Project,
    Codelists,
    Function,
}

impl FileType {
    pub const ALL: [Self; 4] = [Self::Spec, Self::Project, Self::Codelists, Self::Function];
    /// The spelling a document writes in `type`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Spec => "spec",
            Self::Project => "project",
            Self::Codelists => "codelists",
            Self::Function => "function",
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == text)
    }
    /// The class of the 1.0 schema a document of this type is checked against.
    pub fn root_class(self) -> &'static str {
        match self {
            Self::Spec => "root_class",
            Self::Project => "project_class",
            Self::Codelists => "codelist_file_class",
            Self::Function => "function_definition_class",
        }
    }
}

/// Whether a document's root declares the 1.x header: a mapping holding
/// `yamaa` and no `schema_version`. A root that declares `schema_version`
/// keeps the 0.5 path, which reports a `yamaa` beside it as it reports any
/// unknown field (D75).
pub fn declares_header(document: &Document) -> bool {
    let root = document.root();
    document.field(root, "yamaa").is_some() && document.field(root, "schema_version").is_none()
}

/// The header a 1.x document declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: LanguageVersion,
    pub file_type: FileType,
}

/// Why a document's header is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeaderFault {
    /// The root is not a non-empty mapping, `yamaa` or `type` is missing or
    /// not text, or `yamaa` stands beside `schema_version`: a schema finding
    /// in the document, at that field.
    Shape(SchemaDiagnostic),
    /// The document declares a version the engine does not read.
    Version {
        /// The `yamaa` value occurrence in the document.
        node: usize,
        version: String,
        /// The versions the engine reads, oldest first.
        reads: Vec<String>,
    },
    /// The document declares a type its call or referencing field does not expect.
    FileType {
        /// The `type` value occurrence in the document.
        node: usize,
        file_type: String,
        expected: FileType,
    },
}

fn shape(path: &str, code: ConditionCode, context: Vec<(&'static str, C)>) -> HeaderFault {
    HeaderFault::Shape(SchemaDiagnostic {
        path: path.into(),
        code,
        context,
    })
}

/// The value of one header field, or the finding its absence or kind gives.
/// `class` is the root class of the type the reader expects, which a missing
/// field's finding names; a reader that expects no type names none.
fn header_field<'a>(
    document: &'a Document,
    name: &'static str,
    expected: &str,
    class: Option<&str>,
) -> Result<(usize, &'a str), HeaderFault> {
    let Some(node) = document.field(document.root(), name) else {
        let mut context = vec![("field", C::Text(name.into()))];
        context.extend(class.map(|class| ("class", C::Text(class.into()))));
        return Err(shape(
            name,
            ConditionCode::SchemaMissingRequiredField,
            context,
        ));
    };
    match &document.nodes()[node] {
        N::Text(text) => Ok((node, text)),
        other => Err(shape(
            name,
            ConditionCode::SchemaInvalidFieldType,
            vec![
                ("expected", C::Text(expected.into())),
                ("actual", C::ValueKind(other.kind())),
            ],
        )),
    }
}

/// The root must be a non-empty mapping before its header is read; the
/// finding names `class`, the root class of the type the reader expects, as
/// the expected value, when the reader expects one.
fn root_mapping(document: &Document, class: Option<&str>) -> Result<(), HeaderFault> {
    let root = &document.nodes()[document.root()];
    if matches!(root, N::Mapping(entries) if !entries.is_empty()) {
        return Ok(());
    }
    let mut context = Vec::new();
    context.extend(class.map(|class| ("expected", C::Text(class.into()))));
    context.push(("actual", C::ValueKind(root.kind())));
    Err(shape("$", ConditionCode::SchemaInvalidFieldType, context))
}

/// The version `yamaa` declares, when it names one an engine whose newest is
/// `reads` reads.
fn read_version(
    document: &Document,
    reads: Option<LanguageVersion>,
    class: Option<&str>,
) -> Result<LanguageVersion, HeaderFault> {
    let (node, written) = header_field(document, "yamaa", "language_version", class)?;
    LanguageVersion::parse(written)
        .filter(|version| reads.is_some_and(|newest| *version <= newest))
        .ok_or_else(|| HeaderFault::Version {
            node,
            version: written.into(),
            reads: language_versions(reads),
        })
}

/// Read the header of a 1.x document before its schema closure is chosen.
///
/// `reads` is the newest version the engine reads, if it reads any;
/// `expected` is the type the call or referencing field reads. The root must
/// be a non-empty mapping; `yamaa` must be text naming a version the engine
/// reads, else the document fails as `unsupported_language_version` and is
/// not checked further; `type` must be text naming the expected type, else
/// it fails as `unexpected_file_type`. An unquoted `1.0` is a float and fails
/// as `invalid_field_type` (SCHEMA-2, SCHEMA-4).
///
/// A root that declares `schema_version` is a 0.5 document wherever it is
/// read, as [`declares_header`] routes it (D75): a `yamaa` beside it is not a
/// header but the field the 0.5 path reports as `unknown_field`, and it fails
/// so here too, whether a call, a `parent` or a referencing field reads it.
pub fn read_header(
    document: &Document,
    reads: Option<LanguageVersion>,
    expected: FileType,
) -> Result<Header, HeaderFault> {
    let class = expected.root_class();
    root_mapping(document, Some(class))?;
    if !declares_header(document) && document.field(document.root(), "yamaa").is_some() {
        return Err(shape(
            "yamaa",
            ConditionCode::SchemaUnknownField,
            vec![
                ("field", C::Text("yamaa".into())),
                ("class", C::Text(class.into())),
            ],
        ));
    }
    let version = read_version(document, reads, Some(class))?;
    let (node, written) = header_field(document, "type", "str", Some(class))?;
    let file_type = FileType::parse(written)
        .filter(|declared| *declared == expected)
        .ok_or_else(|| HeaderFault::FileType {
            node,
            file_type: written.into(),
            expected,
        })?;
    Ok(Header { version, file_type })
}

impl HeaderFault {
    /// The finding in the document a call reads, at its own `yamaa` or `type`
    /// (SCHEMA-2). Context values address that document.
    pub fn finding(&self) -> SchemaDiagnostic {
        match self {
            Self::Shape(finding) => finding.clone(),
            Self::Version { node, reads, .. } => SchemaDiagnostic {
                path: "yamaa".into(),
                code: ConditionCode::HeaderLanguageVersion,
                context: vec![
                    ("version", C::InputValue(*node)),
                    ("language_versions", C::TextList(reads.clone())),
                ],
            },
            Self::FileType { node, expected, .. } => SchemaDiagnostic {
                path: "type".into(),
                code: ConditionCode::HeaderFileType,
                context: vec![
                    ("type", C::InputValue(*node)),
                    ("expected", C::Text(expected.name().into())),
                ],
            },
        }
    }
}

/// The language a document that declares `schema_version` is written in: the
/// language at the `yamaa-v0.5.0` tag (#3030 D75), which declares no type.
const LEGACY_LANGUAGE_VERSION: &str = "0.5";

/// The type a document's root declares in `type`, when that is text naming
/// one of the four file types.
pub fn declared_type(document: &Document) -> Option<FileType> {
    match &document.nodes()[document.field(document.root(), "type")?] {
        N::Text(text) => FileType::parse(text),
        _ => None,
    }
}

/// Why the type a document declares is not read (#3030 D122).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeFault {
    /// The root declares `schema_version`: a 0.5 document, which declares no
    /// type and which a read of the 1.x header does not read.
    Legacy {
        /// The versions the engine reads, oldest first.
        reads: Vec<String>,
    },
    /// The header is refused as [`read_header`] refuses it.
    Header(HeaderFault),
    /// `type` is text naming none of the four file types.
    Unknown {
        /// The `type` value occurrence in the document.
        node: usize,
        file_type: String,
    },
}

impl TypeFault {
    /// The finding in the document, at its own `schema_version`, `yamaa` or
    /// `type` (SCHEMA-2). Context values address that document.
    pub fn finding(&self) -> SchemaDiagnostic {
        match self {
            Self::Legacy { reads } => SchemaDiagnostic {
                path: "schema_version".into(),
                code: ConditionCode::HeaderLanguageVersion,
                context: vec![
                    ("version", C::Text(LEGACY_LANGUAGE_VERSION.into())),
                    ("language_versions", C::TextList(reads.clone())),
                ],
            },
            Self::Header(fault) => fault.finding(),
            Self::Unknown { node, .. } => SchemaDiagnostic {
                path: "type".into(),
                code: ConditionCode::HeaderFileType,
                context: vec![
                    ("type", C::InputValue(*node)),
                    (
                        "expected",
                        C::TextList(FileType::ALL.map(|kind| kind.name().into()).into()),
                    ),
                ],
            },
        }
    }
}

/// Read the type a document declares, expecting none, by an engine whose
/// newest version is `reads`: what the command line dispatches on before any
/// call reads the file (#3030 D122).
///
/// A root that declares `schema_version`, with `yamaa` or without, is a 0.5
/// document and declares no type: it fails as `unsupported_language_version`
/// at `schema_version`, naming version 0.5. A root that declares one of the
/// four types is read as [`read_header`] reads that type, so it fails as a
/// call that reads that type fails. Any other root is read in the same order,
/// `yamaa` first, then `type`, with findings that name no class, since no
/// closure is chosen; a `type` naming none of the four types fails as
/// `unexpected_file_type`, expecting any of them (SCHEMA-2).
pub fn read_declared_header(
    document: &Document,
    reads: LanguageVersion,
) -> Result<Header, TypeFault> {
    if document.field(document.root(), "schema_version").is_some() {
        return Err(TypeFault::Legacy {
            reads: language_versions(Some(reads)),
        });
    }
    if let Some(declared) = declared_type(document) {
        return read_header(document, Some(reads), declared).map_err(TypeFault::Header);
    }
    root_mapping(document, None).map_err(TypeFault::Header)?;
    read_version(document, Some(reads), None).map_err(TypeFault::Header)?;
    let (node, written) = header_field(document, "type", "str", None).map_err(TypeFault::Header)?;
    Err(TypeFault::Unknown {
        node,
        file_type: written.into(),
    })
}

/// The finding for a file type a call takes as its argument, `written`, that
/// names none of the four types, as the type whose JSON Schema `schema` gives
/// (#3030 D149): `unexpected_file_type`, expecting any of them, as a `type`
/// header naming none of them does (SCHEMA-2). The call reads no file, so the
/// finding has no spec path.
pub fn unknown_file_type(written: &str) -> Diagnostic {
    Diagnostic {
        code: ConditionCode::ArgumentFileType,
        spec_paths: Vec::new(),
        context: Context::from([
            ("type".into(), text(written)),
            (
                "expected".into(),
                JsonValue::Array(FileType::ALL.map(|kind| text(kind.name())).into()),
            ),
        ]),
        source_span: None,
        operand_route: None,
    }
}

fn text(value: &str) -> JsonValue {
    JsonValue::String(value.into())
}

/// The context a version fault reports beside its source.
pub(crate) fn version_context(context: &mut Context, version: &str, reads: &[String]) {
    context.insert("version".into(), text(version));
    context.insert(
        "language_versions".into(),
        JsonValue::Array(reads.iter().map(|version| text(version)).collect()),
    );
}

/// The context a type fault reports beside its source.
pub(crate) fn file_type_context(context: &mut Context, file_type: &str, expected: FileType) {
    context.insert("type".into(), text(file_type));
    context.insert("expected".into(), text(expected.name()));
}

/// The directory a relative path resolves from (FILE-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathBase {
    /// The directory of the file that writes the path.
    Declaring,
    /// The directory of the specification's entry file, which an input
    /// selects with `relative_to: entry` for its `path` and `schema`.
    Entry,
}

/// A field of a 1.x document that names another yamaa file, and the type that
/// file must declare (SCHEMA-2): an input's `schema`, and a project's
/// `functions`, `codelists` and section `specs`. A layer's `parent` is read
/// by the inheritance traversal (LAYER-4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    /// The field's spec path in the referencing document.
    pub field: String,
    /// The path the field writes.
    pub written: String,
    pub expected: FileType,
    /// The directory `written` resolves from: the input's `relative_to` for
    /// a `schema`, and the declaring file for every project field.
    pub base: PathBase,
}

/// Why the file a referencing field names is refused.
#[derive(Clone, Debug, PartialEq)]
pub enum ReferenceFault {
    /// The named file's root is malformed where its header is read: a finding
    /// in that file, at its own field.
    Finding(SchemaDiagnostic),
    /// The named file declares a version the engine does not read or a type
    /// the field does not expect: a diagnostic at the referencing field.
    Field(Diagnostic),
}

fn text_items(document: &Document, node: Option<usize>) -> Vec<(usize, &str)> {
    let Some(N::Sequence(items)) = node.map(|node| &document.nodes()[node]) else {
        return Vec::new();
    };
    items
        .iter()
        .enumerate()
        .filter_map(|(index, &item)| match &document.nodes()[item] {
            N::Text(written) => Some((index, written.as_str())),
            _ => None,
        })
        .collect()
}

fn entries(document: &Document, node: usize) -> &[(usize, usize)] {
    match &document.nodes()[node] {
        N::Mapping(entries) => entries,
        _ => &[],
    }
}

/// The `schema` each input of `inputs` writes, in written order, resolved
/// from the base its `relative_to` selects (FILE-2).
fn schema_links(document: &Document, inputs: usize) -> Vec<Reference> {
    let mut links = Vec::new();
    for &(name, declaration) in entries(document, inputs) {
        let (N::Text(name), Some(schema)) = (
            &document.nodes()[name],
            document.field(declaration, "schema"),
        ) else {
            continue;
        };
        let N::Text(written) = &document.nodes()[schema] else {
            continue;
        };
        let from_entry = document.field(declaration, "relative_to").is_some_and(
            |base| matches!(&document.nodes()[base], N::Text(base) if base == "entry"),
        );
        links.push(Reference {
            field: format!("input.{name}.schema"),
            written: written.clone(),
            expected: FileType::Spec,
            base: if from_entry {
                PathBase::Entry
            } else {
                PathBase::Declaring
            },
        });
    }
    links
}

/// The paths a project field lists, at `prefix[i]`, each naming a file of
/// type `expected` beside the project file.
fn listed(
    document: &Document,
    items: Option<usize>,
    prefix: &str,
    expected: FileType,
) -> Vec<Reference> {
    text_items(document, items)
        .into_iter()
        .map(|(index, written)| Reference {
            field: format!("{prefix}[{index}]"),
            written: written.into(),
            expected,
            base: PathBase::Declaring,
        })
        .collect()
}

/// The referencing fields a 1.x document of `file_type` writes, in the order
/// they are written, which decides the refused file a check reports first.
/// Only written paths are references: an inline codelist source is not.
pub fn references(document: &Document, file_type: FileType) -> Vec<Reference> {
    let mut references = Vec::new();
    for &(key, value) in entries(document, document.root()) {
        let N::Text(field) = &document.nodes()[key] else {
            continue;
        };
        references.extend(match (file_type, field.as_str()) {
            (FileType::Spec, "input") => schema_links(document, value),
            (FileType::Project, "functions") => {
                listed(document, Some(value), field, FileType::Function)
            }
            (FileType::Project, "codelists") => {
                listed(document, Some(value), field, FileType::Codelists)
            }
            (FileType::Project, section @ ("sdtm" | "adam" | "send")) => listed(
                document,
                document.field(value, "specs"),
                &format!("{section}.specs"),
                FileType::Spec,
            ),
            _ => Vec::new(),
        });
    }
    references
}

impl Reference {
    /// Check the header of the file this field names, read from `source` as
    /// `document`, by an engine whose newest version is `reads` (SCHEMA-2).
    /// A named file is routed as [`read_header`] routes every root: one that
    /// declares `schema_version` is a 0.5 document, whose `yamaa` is a
    /// finding in that file.
    pub fn check(
        &self,
        source: &str,
        document: &Document,
        reads: LanguageVersion,
    ) -> Result<Header, ReferenceFault> {
        read_header(document, Some(reads), self.expected)
            .map_err(|fault| self.refuse(source, &fault))
    }

    /// How this field reports the file it names, read from `source`, whose
    /// header `fault` refuses (SCHEMA-2): a malformed header is a finding in
    /// that file, at its own field; a version the engine does not read, or a
    /// type the field does not expect, is a diagnostic at this field, naming
    /// the file as `source`. A reader that decoded the file before it knew the
    /// field, such as a project's preparation of the files it lists, reports
    /// the fault it kept here.
    pub fn refuse(&self, source: &str, fault: &HeaderFault) -> ReferenceFault {
        let mut context = Context::new();
        context.insert("source".into(), text(source));
        let code = match fault {
            HeaderFault::Shape(finding) => return ReferenceFault::Finding(finding.clone()),
            HeaderFault::Version { version, reads, .. } => {
                version_context(&mut context, version, reads);
                ConditionCode::ReferenceLanguageVersion
            }
            HeaderFault::FileType {
                file_type,
                expected,
                ..
            } => {
                file_type_context(&mut context, file_type, *expected);
                ConditionCode::ReferenceFileType
            }
        };
        ReferenceFault::Field(Diagnostic {
            code,
            spec_paths: vec![self.field.clone()],
            context,
            source_span: None,
            operand_route: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_document::Tree as T;

    fn document(tree: T) -> Document {
        tree.document()
    }

    #[test]
    fn versions_are_written_1_n_and_order_by_their_minor_number() {
        for (text, minor) in [
            ("1.0", 0),
            ("1.1", 1),
            ("1.10", 10),
            ("1.4294967295", u32::MAX),
        ] {
            let version = LanguageVersion::parse(text).unwrap();
            assert_eq!(version.minor(), minor);
            assert_eq!(version.to_string(), text);
        }
        for text in [
            "",
            "1",
            "1.",
            "1.00",
            "1.01",
            "01.0",
            "2.0",
            "0.5",
            "1.0 ",
            " 1.0",
            "1.-1",
            "1.+1",
            "1.1.0",
            "1.4294967296",
            "v1.0",
        ] {
            assert_eq!(LanguageVersion::parse(text), None, "{text:?}");
        }
        assert!(LanguageVersion::new(9) < LanguageVersion::new(10));
        assert_eq!(language_versions(None), Vec::<String>::new());
        assert_eq!(language_versions(Some(LANGUAGE_VERSION)), ["1.0"]);
        assert_eq!(
            language_versions(Some(LanguageVersion::new(2))),
            ["1.0", "1.1", "1.2"]
        );
    }

    #[test]
    fn file_types_spell_their_type_and_name_their_root_class() {
        for (kind, name, class) in [
            (FileType::Spec, "spec", "root_class"),
            (FileType::Project, "project", "project_class"),
            (FileType::Codelists, "codelists", "codelist_file_class"),
            (FileType::Function, "function", "function_definition_class"),
        ] {
            assert_eq!((kind.name(), kind.root_class()), (name, class));
            assert_eq!(FileType::parse(name), Some(kind));
        }
        assert_eq!(FileType::parse("environment"), None);
    }

    #[test]
    fn schema_version_keeps_the_0_5_path_and_yamaa_alone_declares_the_header() {
        let both = document(T::map(vec![
            ("schema_version", T::text("1.0")),
            ("yamaa", T::text("1.0")),
        ]));
        assert!(!declares_header(&both));
        let legacy = document(T::map(vec![("schema_version", T::text("1.0"))]));
        assert!(!declares_header(&legacy));
        // A header-less 0.5 function or codelist file, or a scalar root.
        assert!(!declares_header(&document(T::map(vec![(
            "name",
            T::text("f")
        )]))));
        assert!(!declares_header(&document(T::text("yamaa"))));
        let versioned = document(T::map(vec![("yamaa", T::text("1.0"))]));
        assert!(declares_header(&versioned));
    }

    #[test]
    fn the_header_is_read_in_order_before_any_closure() {
        let header = |tree: T, reads, expected| read_header(&document(tree), reads, expected);
        let file = |version: T, kind: T| T::map(vec![("yamaa", version), ("type", kind)]);
        assert_eq!(
            header(
                file(T::text("1.0"), T::text("spec")),
                Some(LANGUAGE_VERSION),
                FileType::Spec
            ),
            Ok(Header {
                version: LanguageVersion::new(0),
                file_type: FileType::Spec,
            })
        );
        // A newer version than the engine reads, and any version when it reads none.
        for (version, reads, listed) in [
            ("1.1", Some(LANGUAGE_VERSION), vec!["1.0"]),
            ("1.0", None, vec![]),
            (
                "2.0",
                Some(LanguageVersion::new(3)),
                vec!["1.0", "1.1", "1.2", "1.3"],
            ),
            ("one", Some(LANGUAGE_VERSION), vec!["1.0"]),
        ] {
            let Err(HeaderFault::Version {
                version: declared,
                reads: read,
                ..
            }) = header(
                file(T::text(version), T::text("project")),
                reads,
                FileType::Spec,
            )
            else {
                panic!("{version} is not read");
            };
            assert_eq!(declared, version);
            assert_eq!(read, listed);
        }
        // The version is refused before the type is read.
        assert!(matches!(
            header(
                T::map(vec![("yamaa", T::text("1.9"))]),
                Some(LANGUAGE_VERSION),
                FileType::Spec
            ),
            Err(HeaderFault::Version { .. })
        ));
        let Err(HeaderFault::FileType {
            file_type,
            expected,
            ..
        }) = header(
            file(T::text("1.0"), T::text("project")),
            Some(LANGUAGE_VERSION),
            FileType::Spec,
        )
        else {
            panic!("a project file where a spec is expected");
        };
        assert_eq!((file_type.as_str(), expected), ("project", FileType::Spec));
        assert!(matches!(
            header(
                file(T::text("1.0"), T::text("codelist")),
                Some(LANGUAGE_VERSION),
                FileType::Codelists
            ),
            Err(HeaderFault::FileType { .. })
        ));
        let shapes = [
            (T::list(vec![]), "$", ConditionCode::SchemaInvalidFieldType),
            (T::map(vec![]), "$", ConditionCode::SchemaInvalidFieldType),
            (
                T::map(vec![("type", T::text("spec"))]),
                "yamaa",
                ConditionCode::SchemaMissingRequiredField,
            ),
            (
                file(T::float(1.0), T::text("spec")),
                "yamaa",
                ConditionCode::SchemaInvalidFieldType,
            ),
            (
                T::map(vec![("yamaa", T::text("1.0"))]),
                "type",
                ConditionCode::SchemaMissingRequiredField,
            ),
            (
                file(T::text("1.0"), T::list(vec![])),
                "type",
                ConditionCode::SchemaInvalidFieldType,
            ),
        ];
        for (tree, path, code) in shapes {
            let Err(HeaderFault::Shape(finding)) =
                header(tree, Some(LANGUAGE_VERSION), FileType::Function)
            else {
                panic!("{path}: a malformed header");
            };
            assert_eq!((finding.path.as_str(), finding.code), (path, code));
        }
        let Err(HeaderFault::Shape(finding)) = header(
            file(T::float(1.0), T::text("spec")),
            Some(LANGUAGE_VERSION),
            FileType::Spec,
        ) else {
            panic!("an unquoted version");
        };
        assert_eq!(
            finding.context,
            [
                ("expected", C::Text("language_version".into())),
                ("actual", C::ValueKind(crate::schema::DocumentKind::Float)),
            ]
        );
    }

    #[test]
    fn an_engine_reads_1_0_and_refuses_a_newer_version_at_yamaa() {
        // The engine reads 1.0 (D76): a 1.0 file of any type is read, and a
        // newer version fails at its own `yamaa`, naming the versions read.
        for kind in FileType::ALL {
            let read = document(T::map(vec![
                ("yamaa", T::text("1.0")),
                ("type", T::text(kind.name())),
            ]));
            assert_eq!(
                read_header(&read, Some(LANGUAGE_VERSION), kind),
                Ok(Header {
                    version: LanguageVersion::new(0),
                    file_type: kind,
                })
            );
        }
        let newer = document(T::map(vec![("yamaa", T::text("1.7"))]));
        let finding = read_header(&newer, Some(LANGUAGE_VERSION), FileType::Spec)
            .unwrap_err()
            .finding();
        assert_eq!(finding.code, ConditionCode::HeaderLanguageVersion);
        assert_eq!(finding.path, "yamaa");
        let node = newer.field(newer.root(), "yamaa").unwrap();
        assert_eq!(
            finding.context,
            [
                ("version", C::InputValue(node)),
                ("language_versions", C::TextList(vec!["1.0".into()])),
            ]
        );
    }

    #[test]
    fn a_yamaa_beside_schema_version_is_an_unknown_field_wherever_it_is_read() {
        // The root routes as `declares_header` routes it: `schema_version`
        // makes it a 0.5 document, where `yamaa` is an unknown field, so a
        // parent or a referencing field reads it as the decoder does.
        for expected in FileType::ALL {
            let both = document(T::map(vec![
                ("schema_version", T::text("1.0")),
                ("yamaa", T::text("1.0")),
                ("type", T::text(expected.name())),
            ]));
            let Err(HeaderFault::Shape(finding)) =
                read_header(&both, Some(LANGUAGE_VERSION), expected)
            else {
                panic!("{expected:?}: a 0.5 document");
            };
            assert_eq!(
                (finding.path.as_str(), finding.code),
                ("yamaa", ConditionCode::SchemaUnknownField)
            );
            assert_eq!(
                finding.context,
                [
                    ("field", C::Text("yamaa".into())),
                    ("class", C::Text(expected.root_class().into())),
                ]
            );
        }
        // A 0.5 document without `yamaa` misses the header.
        let legacy = document(T::map(vec![("schema_version", T::text("1.0"))]));
        let Err(HeaderFault::Shape(finding)) =
            read_header(&legacy, Some(LANGUAGE_VERSION), FileType::Spec)
        else {
            panic!("a 0.5 document");
        };
        assert_eq!(
            (finding.path.as_str(), finding.code),
            ("yamaa", ConditionCode::SchemaMissingRequiredField)
        );
    }

    #[test]
    fn a_1_x_document_declares_its_type_and_a_0_5_document_none() {
        use crate::schema::DocumentKind as K;
        let file = |version: T, kind: T| T::map(vec![("yamaa", version), ("type", kind)]);
        let read = |tree: T| {
            let document = document(tree);
            (read_declared_header(&document, LANGUAGE_VERSION), document)
        };
        // Each type a 1.0 document declares is read without expecting it.
        for kind in FileType::ALL {
            let (header, document) = read(file(T::text("1.0"), T::text(kind.name())));
            assert_eq!(declared_type(&document), Some(kind));
            assert_eq!(
                header,
                Ok(Header {
                    version: LanguageVersion::new(0),
                    file_type: kind,
                })
            );
        }
        // A document that declares `schema_version`, beside `yamaa` or not,
        // declares no type: language version 0.5, at `schema_version`.
        for tree in [
            T::map(vec![
                ("schema_version", T::text("1.0")),
                ("domain", T::text("DM")),
            ]),
            T::map(vec![
                ("schema_version", T::text("1.0")),
                ("yamaa", T::text("1.0")),
                ("type", T::text("spec")),
            ]),
        ] {
            let (header, _) = read(tree);
            let fault = header.unwrap_err();
            assert_eq!(
                fault,
                TypeFault::Legacy {
                    reads: vec!["1.0".into()]
                }
            );
            let finding = fault.finding();
            assert_eq!(
                (finding.path.as_str(), finding.code),
                ("schema_version", ConditionCode::HeaderLanguageVersion)
            );
            assert_eq!(
                finding.context,
                [
                    ("version", C::Text("0.5".into())),
                    ("language_versions", C::TextList(vec!["1.0".into()])),
                ]
            );
        }
        // A document that declares a type it knows is read as a call that
        // reads that type reads it: the version first, then the type, and a
        // finding names that type's class.
        for tree in [
            file(T::text("1.1"), T::text("project")),
            file(T::float(1.0), T::text("function")),
            T::map(vec![("type", T::text("codelists"))]),
        ] {
            let document = document(tree);
            let kind = declared_type(&document).unwrap();
            assert_eq!(
                read_declared_header(&document, LANGUAGE_VERSION).unwrap_err(),
                TypeFault::Header(
                    read_header(&document, Some(LANGUAGE_VERSION), kind).unwrap_err()
                )
            );
        }
        let (header, _) = read(T::map(vec![("type", T::text("project"))]));
        assert_eq!(
            header.unwrap_err().finding().context,
            [
                ("field", C::Text("yamaa".into())),
                ("class", C::Text("project_class".into())),
            ]
        );
        // A type naming none of the four is unexpected where any is expected,
        // once the version is read.
        let (header, document) = read(file(T::text("1.0"), T::text("environment")));
        let node = document.field(document.root(), "type").unwrap();
        let fault = header.unwrap_err();
        assert_eq!(
            fault,
            TypeFault::Unknown {
                node,
                file_type: "environment".into(),
            }
        );
        let finding = fault.finding();
        assert_eq!(
            (finding.path.as_str(), finding.code),
            ("type", ConditionCode::HeaderFileType)
        );
        assert_eq!(
            finding.context,
            [
                ("type", C::InputValue(node)),
                (
                    "expected",
                    C::TextList(vec![
                        "spec".into(),
                        "project".into(),
                        "codelists".into(),
                        "function".into(),
                    ])
                ),
            ]
        );
        let (header, _) = read(file(T::text("1.4"), T::text("codelist")));
        assert!(matches!(
            header,
            Err(TypeFault::Header(HeaderFault::Version { .. }))
        ));
        // Without a type it knows, a malformed header's finding names no class.
        let shapes = [
            (
                T::list(vec![]),
                "$",
                ConditionCode::SchemaInvalidFieldType,
                vec![("actual", C::ValueKind(K::Sequence))],
            ),
            (
                T::map(vec![]),
                "$",
                ConditionCode::SchemaInvalidFieldType,
                vec![("actual", C::ValueKind(K::Mapping))],
            ),
            (
                T::map(vec![("domain", T::text("DM"))]),
                "yamaa",
                ConditionCode::SchemaMissingRequiredField,
                vec![("field", C::Text("yamaa".into()))],
            ),
            (
                file(T::float(1.0), T::text("specification")),
                "yamaa",
                ConditionCode::SchemaInvalidFieldType,
                vec![
                    ("expected", C::Text("language_version".into())),
                    ("actual", C::ValueKind(K::Float)),
                ],
            ),
            (
                T::map(vec![("yamaa", T::text("1.0"))]),
                "type",
                ConditionCode::SchemaMissingRequiredField,
                vec![("field", C::Text("type".into()))],
            ),
            (
                file(T::text("1.0"), T::list(vec![])),
                "type",
                ConditionCode::SchemaInvalidFieldType,
                vec![
                    ("expected", C::Text("str".into())),
                    ("actual", C::ValueKind(K::Sequence)),
                ],
            ),
        ];
        for (tree, path, code, context) in shapes {
            let (header, _) = read(tree);
            let Err(TypeFault::Header(HeaderFault::Shape(finding))) = header else {
                panic!("{path}: a malformed header");
            };
            assert_eq!((finding.path.as_str(), finding.code), (path, code));
            assert_eq!(finding.context, context, "{path}");
        }
    }

    #[test]
    fn referencing_fields_name_the_type_their_file_declares() {
        let spec = document(T::map(vec![
            ("yamaa", T::text("1.0")),
            ("parent", T::text("base.yaml")),
            (
                "input",
                T::map(vec![
                    ("RAW", T::text("raw.csv")),
                    (
                        "DM",
                        T::map(vec![
                            ("path", T::text("dm.csv")),
                            ("schema", T::text("dm.yaml")),
                        ]),
                    ),
                    ("AE", T::map(vec![("path", T::text("ae.csv"))])),
                    (
                        "SV",
                        T::map(vec![
                            ("path", T::text("sv.csv")),
                            ("relative_to", T::text("entry")),
                            ("schema", T::text("sv.yaml")),
                        ]),
                    ),
                ]),
            ),
        ]));
        assert_eq!(
            references(&spec, FileType::Spec),
            [
                Reference {
                    field: "input.DM.schema".into(),
                    written: "dm.yaml".into(),
                    expected: FileType::Spec,
                    base: PathBase::Declaring,
                },
                // `relative_to` selects the entry's directory for `schema`
                // as for `path` (FILE-2).
                Reference {
                    field: "input.SV.schema".into(),
                    written: "sv.yaml".into(),
                    expected: FileType::Spec,
                    base: PathBase::Entry,
                },
            ]
        );
        let project = document(T::map(vec![
            (
                "functions",
                T::list(vec![T::text("f.yaml"), T::text("g.yaml")]),
            ),
            (
                "codelists",
                T::list(vec![
                    T::map(vec![("codelists", T::list(vec![]))]),
                    T::text("ct.yaml"),
                ]),
            ),
            (
                "adam",
                T::map(vec![("specs", T::list(vec![T::text("adsl.yaml")]))]),
            ),
            (
                "sdtm",
                T::map(vec![("specs", T::list(vec![T::text("dm.yaml")]))]),
            ),
        ]));
        let found = references(&project, FileType::Project)
            .into_iter()
            .map(|reference| {
                assert_eq!(reference.base, PathBase::Declaring);
                (reference.field, reference.written, reference.expected)
            })
            .collect::<Vec<_>>();
        // In the order the fields are written: `adam` before `sdtm` here.
        assert_eq!(
            found,
            [
                ("functions[0]".into(), "f.yaml".into(), FileType::Function),
                ("functions[1]".into(), "g.yaml".into(), FileType::Function),
                ("codelists[1]".into(), "ct.yaml".into(), FileType::Codelists),
                ("adam.specs[0]".into(), "adsl.yaml".into(), FileType::Spec),
                ("sdtm.specs[0]".into(), "dm.yaml".into(), FileType::Spec),
            ]
        );
        let reordered = document(T::map(vec![
            (
                "send",
                T::map(vec![("specs", T::list(vec![T::text("tx.yaml")]))]),
            ),
            ("codelists", T::list(vec![T::text("ct.yaml")])),
            ("functions", T::list(vec![T::text("f.yaml")])),
        ]));
        let fields = references(&reordered, FileType::Project)
            .into_iter()
            .map(|reference| reference.field)
            .collect::<Vec<_>>();
        assert_eq!(fields, ["send.specs[0]", "codelists[0]", "functions[0]"]);
        assert!(references(&project, FileType::Function).is_empty());
        // A specification's fields are not a project's.
        assert!(references(&reordered, FileType::Spec).is_empty());
    }
}
