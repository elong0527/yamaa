//! The header a yamaa document's root declares: the language version the
//! document needs and its file type (SCHEMA-2, LAYER-4, P-VER-1).
//!
//! A root that declares `schema_version`, or no `yamaa`, is read by the 0.5
//! language at the `yamaa-v0.5.0` tag (#3030 D75). A root that declares `yamaa`
//! is a 1.x document: its header is read here, before any schema closure is
//! chosen, and the declared type then selects the 1.0 closure. Until the engine
//! reads 1.0 files (#3030 D76, P1-9) the 0.5 path refuses a 1.x document with
//! [`refusal`], so only tests reach the 1.0 path.
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
    /// The root is not a non-empty mapping, or `yamaa` or `type` is missing or
    /// not text: a schema finding in the document, at that field.
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
fn header_field<'a>(
    document: &'a Document,
    name: &'static str,
    expected: &str,
    class: &str,
) -> Result<(usize, &'a str), HeaderFault> {
    let Some(node) = document.field(document.root(), name) else {
        return Err(shape(
            name,
            ConditionCode::SchemaMissingRequiredField,
            vec![
                ("field", C::Text(name.into())),
                ("class", C::Text(class.into())),
            ],
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

/// Read the header of a 1.x document before its schema closure is chosen.
///
/// `reads` is the newest version the engine reads, if it reads any;
/// `expected` is the type the call or referencing field reads. The root must
/// be a non-empty mapping; `yamaa` must be text naming a version the engine
/// reads, else the document fails as `unsupported_language_version` and is
/// not checked further; `type` must be text naming the expected type, else
/// it fails as `unexpected_file_type`. An unquoted `1.0` is a float and fails
/// as `invalid_field_type` (SCHEMA-2, SCHEMA-4).
pub fn read_header(
    document: &Document,
    reads: Option<LanguageVersion>,
    expected: FileType,
) -> Result<Header, HeaderFault> {
    let class = expected.root_class();
    let root = &document.nodes()[document.root()];
    if !matches!(root, N::Mapping(entries) if !entries.is_empty()) {
        return Err(shape(
            "$",
            ConditionCode::SchemaInvalidFieldType,
            vec![
                ("expected", C::Text(class.into())),
                ("actual", C::ValueKind(root.kind())),
            ],
        ));
    }
    let (node, written) = header_field(document, "yamaa", "language_version", class)?;
    let version = LanguageVersion::parse(written)
        .filter(|version| reads.is_some_and(|newest| *version <= newest))
        .ok_or_else(|| HeaderFault::Version {
            node,
            version: written.into(),
            reads: language_versions(reads),
        })?;
    let (node, written) = header_field(document, "type", "str", class)?;
    let file_type = FileType::parse(written)
        .filter(|declared| *declared == expected)
        .ok_or_else(|| HeaderFault::FileType {
            node,
            file_type: written.into(),
            expected,
        })?;
    Ok(Header { version, file_type })
}

/// The refusal a 0.5 path gives a document that declares the 1.x header,
/// which it does not read: until P1-9 the engine reads no 1.x version
/// (#3030 D76), so every declared version fails as
/// `unsupported_language_version`, and an unquoted `yamaa` as
/// `invalid_field_type`. `None` for every other document, which the 0.5 path
/// reads unchanged.
pub fn refusal(document: &Document) -> Option<HeaderFault> {
    if !declares_header(document) {
        return None;
    }
    // A root that declares the header is a mapping holding `yamaa`, and an
    // engine that reads no version refuses that value before `type` is read,
    // so the expected type is never consulted.
    read_header(document, None, FileType::Spec).err()
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

/// The referencing fields a 1.x document of `file_type` writes, in document
/// order. Only written paths are references: an inline codelist source is not.
pub fn references(document: &Document, file_type: FileType) -> Vec<Reference> {
    let root = document.root();
    let reference = |field: String, written: &str, expected| Reference {
        field,
        written: written.into(),
        expected,
    };
    let mut references = Vec::new();
    match file_type {
        FileType::Spec => {
            let inputs = match document
                .field(root, "input")
                .map(|node| &document.nodes()[node])
            {
                Some(N::Mapping(inputs)) => inputs.as_slice(),
                _ => &[],
            };
            for &(name, declaration) in inputs {
                let (N::Text(name), Some(schema)) = (
                    &document.nodes()[name],
                    document.field(declaration, "schema"),
                ) else {
                    continue;
                };
                if let N::Text(written) = &document.nodes()[schema] {
                    references.push(reference(
                        format!("input.{name}.schema"),
                        written,
                        FileType::Spec,
                    ));
                }
            }
        }
        FileType::Project => {
            for (field, expected) in [
                ("functions", FileType::Function),
                ("codelists", FileType::Codelists),
            ] {
                for (index, written) in text_items(document, document.field(root, field)) {
                    references.push(reference(format!("{field}[{index}]"), written, expected));
                }
            }
            for section in ["sdtm", "adam", "send"] {
                let specs = document
                    .field(root, section)
                    .and_then(|node| document.field(node, "specs"));
                for (index, written) in text_items(document, specs) {
                    references.push(reference(
                        format!("{section}.specs[{index}]"),
                        written,
                        FileType::Spec,
                    ));
                }
            }
        }
        FileType::Codelists | FileType::Function => {}
    }
    references
}

impl Reference {
    /// Check the header of the file this field names, read from `source` as
    /// `document`, by an engine whose newest version is `reads` (SCHEMA-2).
    pub fn check(
        &self,
        source: &str,
        document: &Document,
        reads: LanguageVersion,
    ) -> Result<Header, ReferenceFault> {
        let fault = match read_header(document, Some(reads), self.expected) {
            Ok(header) => return Ok(header),
            Err(HeaderFault::Shape(finding)) => return Err(ReferenceFault::Finding(finding)),
            Err(fault) => fault,
        };
        let mut context = Context::new();
        context.insert("source".into(), text(source));
        let code = match &fault {
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
            HeaderFault::Shape(_) => unreachable!("returned above"),
        };
        Err(ReferenceFault::Field(Diagnostic {
            code,
            spec_paths: vec![self.field.clone()],
            context,
            source_span: None,
            operand_route: None,
        }))
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
        assert_eq!(refusal(&both), None);
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
    fn the_0_5_path_refuses_every_declared_version_before_the_switch() {
        for tree in [
            T::map(vec![("yamaa", T::text("1.0")), ("type", T::text("spec"))]),
            T::map(vec![
                ("yamaa", T::text("1.0")),
                ("type", T::text("project")),
            ]),
            T::map(vec![("yamaa", T::text("1.7"))]),
        ] {
            let refused = document(tree);
            let fault = refusal(&refused).unwrap();
            let finding = fault.finding();
            assert_eq!(finding.code, ConditionCode::HeaderLanguageVersion);
            assert_eq!(finding.path, "yamaa");
            let node = refused.field(refused.root(), "yamaa").unwrap();
            assert_eq!(
                finding.context,
                [
                    ("version", C::InputValue(node)),
                    ("language_versions", C::TextList(Vec::new())),
                ]
            );
        }
        let unquoted = document(T::map(vec![("yamaa", T::float(1.0))]));
        assert_eq!(
            refusal(&unquoted).unwrap().finding().code,
            ConditionCode::SchemaInvalidFieldType
        );
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
                ]),
            ),
        ]));
        assert_eq!(
            references(&spec, FileType::Spec),
            [Reference {
                field: "input.DM.schema".into(),
                written: "dm.yaml".into(),
                expected: FileType::Spec,
            }]
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
            .map(|reference| (reference.field, reference.written, reference.expected))
            .collect::<Vec<_>>();
        assert_eq!(
            found,
            [
                ("functions[0]".into(), "f.yaml".into(), FileType::Function),
                ("functions[1]".into(), "g.yaml".into(), FileType::Function),
                ("codelists[1]".into(), "ct.yaml".into(), FileType::Codelists),
                ("sdtm.specs[0]".into(), "dm.yaml".into(), FileType::Spec),
                ("adam.specs[0]".into(), "adsl.yaml".into(), FileType::Spec),
            ]
        );
        assert!(references(&project, FileType::Function).is_empty());
    }
}
