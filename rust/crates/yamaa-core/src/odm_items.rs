//! The fixed clinical-item table read from CDISC ODM XML: its columns, the
//! reader options and the typed refusals. Reading files is an adapter
//! boundary; this module performs no I/O. Each refusal reports the issue
//! [`crate::diagnostic::OdmReadFinding`] gives it (ODM-1).
//!
//! Every column uses a language column type, so ordinary engine inputs can
//! read the table. `SourceOrdinal` is an integer counting the emitted rows
//! from one in source order, `ValuePresent` and `IsNull` hold the text
//! `true` or `false`, and every other column is nullable text. `Value` keeps
//! absent (missing, `ValuePresent` false), explicitly null (`IsNull` true) and
//! empty (`""`) items distinct; no clinical value is inferred or repaired.
use crate::{
    table::{Column, TableSchema},
    value::ColumnType,
};

/// The clinical-item columns in table order.
pub const COLUMNS: [&str; 30] = [
    "ODMVersion",
    "FileOID",
    "StudyOID",
    "MetaDataVersionOID",
    "SubjectKey",
    "StudySubjectID",
    "SubjectStatus",
    "StudyEventOID",
    "StudyEventRepeatKey",
    "EventName",
    "StartDate",
    "EventStatus",
    "EventWorkflowStatus",
    "FormOID",
    "FormRepeatKey",
    "FormName",
    "FormLayoutOID",
    "FormStatus",
    "FormWorkflowStatus",
    "OpenQueries",
    "ItemGroupOID",
    "ItemGroupRepeatKey",
    "ItemGroupName",
    "TransactionType",
    "ItemOID",
    "ItemName",
    "Value",
    "ValuePresent",
    "IsNull",
    "SourceOrdinal",
];

/// Position of `SourceOrdinal`, the last column and the only integer one.
pub const SOURCE_ORDINAL: usize = COLUMNS.len() - 1;

/// The text spelling of a `ValuePresent` or `IsNull` flag.
pub fn flag(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

/// The language column type of the column at `index` in [`COLUMNS`].
pub fn column_type(index: usize) -> Option<ColumnType> {
    match index {
        SOURCE_ORDINAL => Some(ColumnType::Int),
        index if index < SOURCE_ORDINAL => Some(ColumnType::Str),
        _ => None,
    }
}

/// The ordered table schema of every clinical-item table, empty or not.
pub fn schema() -> TableSchema {
    TableSchema::new(
        COLUMNS
            .iter()
            .enumerate()
            .map(|(index, name)| Column {
                name: (*name).into(),
                kind: column_type(index).expect("fixed clinical-item column"),
            })
            .collect(),
    )
    .expect("fixed clinical-item columns are unique and named")
}

/// Bounds and archive selection for one read. Every limit is inclusive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// The exact TAR member name to read. Without it, a TAR input must hold
    /// exactly one `.xml` member outside macOS metadata.
    pub archive_member: Option<String>,
    /// Members a TAR input may contain, counting every member.
    pub max_archive_members: usize,
    /// Bytes a plain XML file or the selected TAR member may hold, and bytes
    /// a compressed archive may expand to across all of its members.
    pub max_expanded_bytes: u64,
    /// Element nesting depth, counting every element.
    pub max_xml_depth: usize,
}

impl Options {
    pub const DEFAULT_MAX_ARCHIVE_MEMBERS: usize = 10_000;
    pub const DEFAULT_MAX_EXPANDED_BYTES: u64 = 256 * 1024 * 1024;
    pub const DEFAULT_MAX_XML_DEPTH: usize = 64;
    pub const MIN_ARCHIVE_MEMBERS: u64 = 1;
    pub const MIN_EXPANDED_BYTES: u64 = 1;
    /// The smallest depth that admits the clinical hierarchy below `ODM`.
    pub const MIN_XML_DEPTH: usize = 8;

    /// Refuse a bound that no document could satisfy, in a fixed order.
    pub fn validate(&self) -> Result<(), Error> {
        if self.max_expanded_bytes < Self::MIN_EXPANDED_BYTES {
            return Err(Error::Option {
                option: "max_expanded_bytes",
                minimum: Self::MIN_EXPANDED_BYTES,
                value: self.max_expanded_bytes,
            });
        }
        if (self.max_archive_members as u64) < Self::MIN_ARCHIVE_MEMBERS {
            return Err(Error::Option {
                option: "max_archive_members",
                minimum: Self::MIN_ARCHIVE_MEMBERS,
                value: self.max_archive_members as u64,
            });
        }
        if self.max_xml_depth < Self::MIN_XML_DEPTH {
            return Err(Error::Option {
                option: "max_xml_depth",
                minimum: Self::MIN_XML_DEPTH as u64,
                value: self.max_xml_depth as u64,
            });
        }
        Ok(())
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            archive_member: None,
            max_archive_members: Self::DEFAULT_MAX_ARCHIVE_MEMBERS,
            max_expanded_bytes: Self::DEFAULT_MAX_EXPANDED_BYTES,
            max_xml_depth: Self::DEFAULT_MAX_XML_DEPTH,
        }
    }
}

/// A read refusal: an option no document could satisfy, a source the host
/// cannot read, or a source that is not an ODM snapshot the table can hold.
/// The message names the cause, and [`crate::diagnostic::OdmReadFinding`]
/// gives the issue each refusal reports: the source as given and its
/// registered condition (ODM-1). The source path itself is not part of a
/// refusal.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// An option below its minimum; `value` is the option as given.
    #[error("{option} must be at least {minimum}")]
    Option {
        option: &'static str,
        minimum: u64,
        value: u64,
    },
    /// The host failed to read the opened source; `reason` is its text.
    #[error("cannot read ODM source: {reason}")]
    Io { reason: String },
    #[error("cannot read ODM archive: {reason}")]
    Archive { reason: &'static str },
    #[error(
        "ODM archive uses {format} compression; only plain and gzip TAR archives are supported"
    )]
    Compression { format: &'static str },
    #[error("{0}")]
    Member(Member),
    #[error("{0}")]
    Limit(Limit),
    #[error("malformed ODM XML: {reason} at byte {position}")]
    Malformed { position: u64, reason: String },
    #[error("DOCTYPE declarations are not supported")]
    Doctype,
    #[error("ODM XML must be encoded as UTF-8, not {encoding}")]
    Encoding { encoding: String },
    #[error("{0}")]
    Document(Document),
    #[error("{0}")]
    Layout(Layout),
    #[error("{element} requires non-empty @{attribute}")]
    MissingAttribute {
        element: &'static str,
        attribute: &'static str,
    },
    #[error("multiple namespaced attributes have local name {name:?}")]
    AmbiguousAttribute { name: &'static str },
    /// An enumerated attribute holds a value outside its closed list:
    /// `IsNull`, the root's `FileType` or an element's `TransactionType`.
    #[error("unsupported {attribute} value {value:?} on {element}")]
    InvalidAttribute {
        element: String,
        attribute: &'static str,
        value: String,
    },
    #[error("conflicting {} definition for {study}/{version}/{oid}", kind.as_str())]
    MetadataConflict {
        kind: Definition,
        study: String,
        version: String,
        oid: String,
    },
    #[error("ItemData {item:?} {refusal}")]
    Value { item: String, refusal: ValueRefusal },
    /// The document is not a snapshot: a `FileType` of `Transactional` on the
    /// root, or a `TransactionType` of `Remove` on an element.
    #[error("{element} declares {attribute} {value}, so the document is not a snapshot")]
    Transactional {
        element: String,
        attribute: &'static str,
        value: String,
    },
    /// The document holds no `ClinicalData` element, as a define.xml or a
    /// metadata-only export does.
    #[error("the document has no ClinicalData")]
    NoClinicalData,
}

/// The `TransactionType` values ODM defines. Every one but `Remove` reads as
/// the present value.
pub const TRANSACTION_TYPES: [&str; 5] = ["Insert", "Update", "Remove", "Upsert", "Context"];

/// The elements a read skips with everything inside them: their content
/// gives no row and is not checked beyond XML itself and the read limits.
pub const SKIPPED_ELEMENTS: [&str; 5] = [
    "AuditRecord",
    "Signature",
    "Annotation",
    "ReferenceData",
    "AdminData",
];

/// Admit the root's `FileType`: absent or `Snapshot` reads, `Transactional`
/// is not a snapshot, and any other value is not one ODM defines.
pub fn admit_file_type(value: Option<&str>) -> Result<(), Error> {
    match value {
        None | Some("Snapshot") => Ok(()),
        Some(value) if value == "Transactional" => Err(Error::Transactional {
            element: "ODM".into(),
            attribute: "FileType",
            value: value.into(),
        }),
        Some(value) => Err(Error::InvalidAttribute {
            element: "ODM".into(),
            attribute: "FileType",
            value: value.into(),
        }),
    }
}

/// Admit the `TransactionType` of `element`: absent or any defined value
/// but `Remove` reads; `Remove` is not a snapshot, and any other value is
/// not one ODM defines. Values compare exactly, as XML names do.
pub fn admit_transaction_type(element: &str, value: Option<&str>) -> Result<(), Error> {
    match value {
        None => Ok(()),
        Some("Remove") => Err(Error::Transactional {
            element: element.into(),
            attribute: "TransactionType",
            value: "Remove".into(),
        }),
        Some(value) if TRANSACTION_TYPES.contains(&value) => Ok(()),
        Some(value) => Err(Error::InvalidAttribute {
            element: element.into(),
            attribute: "TransactionType",
            value: value.into(),
        }),
    }
}

/// TAR member selection refusals.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Member {
    #[error("archive_member requires a TAR input")]
    RequiresArchive,
    #[error("TAR input must contain exactly one XML member or receive archive_member")]
    Candidates { count: usize },
    #[error("TAR input contains {count} entries named {name:?}")]
    Matches { name: String, count: usize },
    #[error("unsafe TAR member path {name:?}")]
    Unsafe { name: String },
    #[error("TAR member {name:?} is not a regular file")]
    Irregular { name: String },
    #[error("TAR member {name:?} is a sparse file, which is not supported")]
    Sparse { name: String },
}

/// Exceeded size, count and nesting bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Limit {
    #[error("TAR input contains more than {0} members")]
    ArchiveMembers(usize),
    #[error("TAR member exceeds {0} bytes")]
    MemberBytes(u64),
    #[error("compressed TAR input expands beyond {0} bytes")]
    ExpandedBytes(u64),
    #[error("ODM XML exceeds {0} bytes")]
    XmlBytes(u64),
    /// A source a caller's resolver serves holds more than the per-file
    /// source bound, which applies before it is read as XML or TAR.
    #[error("ODM source exceeds {0} bytes")]
    SourceBytes(u64),
    #[error("XML depth exceeds {0}")]
    Depth(usize),
    #[error("XML {token} exceeds {limit} bytes")]
    Token { token: &'static str, limit: usize },
    #[error("XML namespace declarations in scope exceed {0}")]
    Namespaces(usize),
    #[error("ODM items exceed {0} table rows")]
    Rows(usize),
    #[error("ODM column {column} exceeds {limit} bytes in one table batch")]
    TextColumn { column: &'static str, limit: usize },
}

/// Documents outside the supported ODM root.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Document {
    #[error("document root must be ODM")]
    Root,
    #[error("unsupported ODM namespace {}", namespace_text(.namespace.as_deref()))]
    Namespace { namespace: Option<String> },
    #[error("ODMVersion {version:?} conflicts with namespace")]
    Version { version: String },
    #[error("document contains a nested ODM element")]
    NestedOdm,
}

fn namespace_text(namespace: Option<&str>) -> String {
    namespace.map_or_else(|| "(none)".into(), |namespace| format!("{namespace:?}"))
}

/// Clinical layouts the fixed table cannot represent.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Layout {
    #[error("nested {0} is not supported")]
    Nested(&'static str),
    #[error("MetaDataVersion appears outside Study")]
    VersionOutsideStudy,
    #[error("{0} appears outside MetaDataVersion")]
    DefinitionOutsideVersion(&'static str),
    #[error("ItemData requires subject and event context")]
    ItemContext,
    #[error(
        "ItemData requires FormData plus one ItemGroupData, or two ItemGroupData ancestors without FormData"
    )]
    ItemProjection,
    #[error("{0} inside ItemData is not supported")]
    InsideItemData(&'static str),
    #[error("typed ItemData element {0} is not supported; use ItemData")]
    TypedItemData(String),
}

/// Item values the fixed table cannot represent without loss.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ValueRefusal {
    #[error("Value child is in an unsupported namespace")]
    ForeignChild,
    #[error("Value attribute is in an unsupported namespace")]
    ForeignAttribute,
    #[error("contains text outside its value")]
    TextOutside,
    #[error("has attribute and child values")]
    AttributeAndChild,
    #[error("has multiple Value children")]
    MultipleChildren,
    #[error("Value contains child elements")]
    ChildElements,
    #[error("is null and contains a value")]
    NullWithValue,
}

/// Metadata definitions that supply names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Definition {
    Event,
    Form,
    Group,
    Item,
}

impl Definition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::Form => "form",
            Self::Group => "group",
            Self::Item => "item",
        }
    }
}

/// Decode an `ItemData`'s `IsNull`: `yes`, `true` or `1` and `no`, `false`
/// or `0`, compared ASCII case-insensitively; an absent attribute is false.
pub fn parse_is_null(value: Option<&str>) -> Result<bool, Error> {
    let Some(value) = value else {
        return Ok(false);
    };
    if ["yes", "true", "1"]
        .iter()
        .any(|spelling| value.eq_ignore_ascii_case(spelling))
    {
        Ok(true)
    } else if ["no", "false", "0"]
        .iter()
        .any(|spelling| value.eq_ignore_ascii_case(spelling))
    {
        Ok(false)
    } else {
        Err(Error::InvalidAttribute {
            element: "ItemData".into(),
            attribute: "IsNull",
            value: value.into(),
        })
    }
}

/// Whether `version` agrees with the version a supported namespace states:
/// the same version, or a dotted numeric sub-version of it.
pub fn version_matches(version: &str, expected: &str) -> bool {
    if version == expected {
        return true;
    }
    version
        .strip_prefix(expected)
        .and_then(|rest| rest.strip_prefix('.'))
        .is_some_and(|rest| {
            rest.split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_holds_thirty_language_columns_in_fixed_order() {
        let schema = schema();
        let columns = schema.columns();
        assert_eq!(columns.len(), 30);
        for (index, column) in columns.iter().enumerate() {
            assert_eq!(column.name, COLUMNS[index]);
            let expected = if column.name == "SourceOrdinal" {
                ColumnType::Int
            } else {
                ColumnType::Str
            };
            assert_eq!(column.kind, expected, "{}", column.name);
        }
        assert_eq!(COLUMNS[SOURCE_ORDINAL], "SourceOrdinal");
        assert_eq!(column_type(COLUMNS.len()), None);
        assert_eq!((flag(true), flag(false)), ("true", "false"));
    }

    #[test]
    fn canonical_odm_source_fields_are_clinical_item_columns() {
        for field in crate::odm_source::FIELDS {
            let index = COLUMNS
                .iter()
                .position(|column| *column == field)
                .unwrap_or_else(|| panic!("{field} is not a clinical-item column"));
            assert_eq!(column_type(index), Some(ColumnType::Str), "{field}");
        }
    }

    #[test]
    fn options_default_and_refuse_unsatisfiable_bounds_in_order() {
        let options = Options::default();
        assert_eq!(options.archive_member, None);
        assert_eq!(options.max_archive_members, 10_000);
        assert_eq!(options.max_expanded_bytes, 268_435_456);
        assert_eq!(options.max_xml_depth, 64);
        assert_eq!(options.validate(), Ok(()));
        let edge = Options {
            max_archive_members: 1,
            max_expanded_bytes: 1,
            max_xml_depth: 8,
            ..Options::default()
        };
        assert_eq!(edge.validate(), Ok(()));
        let all_invalid = Options {
            max_archive_members: 0,
            max_expanded_bytes: 0,
            max_xml_depth: 7,
            ..Options::default()
        };
        let error = all_invalid.validate().unwrap_err();
        assert_eq!(
            error,
            Error::Option {
                option: "max_expanded_bytes",
                minimum: 1,
                value: 0,
            }
        );
        assert_eq!(error.to_string(), "max_expanded_bytes must be at least 1");
        let members = Options {
            max_archive_members: 0,
            ..Options::default()
        };
        assert_eq!(
            members.validate().unwrap_err().to_string(),
            "max_archive_members must be at least 1"
        );
        let depth = Options {
            max_xml_depth: 7,
            ..Options::default()
        };
        assert_eq!(
            depth.validate().unwrap_err().to_string(),
            "max_xml_depth must be at least 8"
        );
    }

    #[test]
    fn is_null_admits_six_spellings_case_insensitively_and_nothing_else() {
        for (value, expected) in [
            (None, false),
            (Some("yes"), true),
            (Some("Yes"), true),
            (Some("TRUE"), true),
            (Some("1"), true),
            (Some("no"), false),
            (Some("No"), false),
            (Some("fAlSe"), false),
            (Some("0"), false),
        ] {
            assert_eq!(parse_is_null(value), Ok(expected), "{value:?}");
        }
        // Unicode case folding would map the long s to `s`; only ASCII folds.
        for value in [
            "",
            " yes",
            "y",
            "2",
            "ye\u{17f}",
            "fal\u{17f}e",
            "null",
            "\u{ff39}es",
        ] {
            let error = parse_is_null(Some(value)).unwrap_err();
            assert_eq!(
                error,
                Error::InvalidAttribute {
                    element: "ItemData".into(),
                    attribute: "IsNull",
                    value: value.into(),
                }
            );
            assert_eq!(
                error.to_string(),
                format!("unsupported IsNull value {value:?} on ItemData")
            );
        }
    }

    #[test]
    fn a_snapshot_reads_and_transactional_or_unknown_types_are_refused() {
        assert_eq!(admit_file_type(None), Ok(()));
        assert_eq!(admit_file_type(Some("Snapshot")), Ok(()));
        assert_eq!(
            admit_file_type(Some("Transactional")),
            Err(Error::Transactional {
                element: "ODM".into(),
                attribute: "FileType",
                value: "Transactional".into(),
            })
        );
        // Values compare exactly, as XML attribute values do.
        for value in ["snapshot", "Archive", ""] {
            assert_eq!(
                admit_file_type(Some(value)),
                Err(Error::InvalidAttribute {
                    element: "ODM".into(),
                    attribute: "FileType",
                    value: value.into(),
                }),
                "{value:?}"
            );
        }
        for value in [None, Some("Insert"), Some("Update"), Some("Upsert")] {
            assert_eq!(admit_transaction_type("FormData", value), Ok(()));
        }
        assert_eq!(admit_transaction_type("ItemData", Some("Context")), Ok(()));
        let removed = admit_transaction_type("ItemGroupData", Some("Remove")).unwrap_err();
        assert_eq!(
            removed,
            Error::Transactional {
                element: "ItemGroupData".into(),
                attribute: "TransactionType",
                value: "Remove".into(),
            }
        );
        assert_eq!(
            removed.to_string(),
            "ItemGroupData declares TransactionType Remove, so the document is not a snapshot"
        );
        for value in ["remove", "Delete", ""] {
            assert_eq!(
                admit_transaction_type("SubjectData", Some(value)),
                Err(Error::InvalidAttribute {
                    element: "SubjectData".into(),
                    attribute: "TransactionType",
                    value: value.into(),
                }),
                "{value:?}"
            );
        }
        assert!(TRANSACTION_TYPES.contains(&"Remove"));
    }

    #[test]
    fn versions_match_exactly_or_by_dotted_numeric_sub_version() {
        for (version, expected) in [
            ("1.3", "1.3"),
            ("1.3.1", "1.3"),
            ("1.3.2", "1.3"),
            ("2.0", "2.0"),
            ("2.0.1", "2.0"),
            ("2.0.1.2", "2.0"),
        ] {
            assert!(version_matches(version, expected), "{version}");
        }
        for (version, expected) in [
            ("1.3junk", "1.3"),
            ("1.31", "1.3"),
            ("1.3.junk", "1.3"),
            ("1.3.", "1.3"),
            ("2.0junk", "2.0"),
            ("2.01", "2.0"),
            ("2.0.x", "2.0"),
            ("2.0.1a", "2.0"),
            ("2.0..1", "2.0"),
            ("2.0.\u{ff11}", "2.0"),
            ("2.0", "1.3"),
        ] {
            assert!(!version_matches(version, expected), "{version}");
        }
    }

    #[test]
    fn every_refusal_names_its_cause() {
        let cases = [
            (
                Error::Io {
                    reason: "denied".into(),
                },
                "cannot read ODM source: denied",
            ),
            (
                Error::Archive {
                    reason: "truncated TAR header",
                },
                "cannot read ODM archive: truncated TAR header",
            ),
            (
                Error::Compression { format: "xz" },
                "ODM archive uses xz compression; only plain and gzip TAR archives are supported",
            ),
            (
                Error::Member(Member::RequiresArchive),
                "archive_member requires a TAR input",
            ),
            (Error::Limit(Limit::Depth(64)), "XML depth exceeds 64"),
            (
                Error::Malformed {
                    position: 3,
                    reason: "unclosed element".into(),
                },
                "malformed ODM XML: unclosed element at byte 3",
            ),
            (Error::Doctype, "DOCTYPE declarations are not supported"),
            (
                Error::Encoding {
                    encoding: "UTF-16".into(),
                },
                "ODM XML must be encoded as UTF-8, not UTF-16",
            ),
            (Error::Document(Document::Root), "document root must be ODM"),
            (
                Error::Layout(Layout::ItemContext),
                "ItemData requires subject and event context",
            ),
            (
                Error::MissingAttribute {
                    element: "ItemData",
                    attribute: "ItemOID",
                },
                "ItemData requires non-empty @ItemOID",
            ),
            (
                Error::AmbiguousAttribute { name: "ItemName" },
                "multiple namespaced attributes have local name \"ItemName\"",
            ),
            (
                Error::InvalidAttribute {
                    element: "ODM".into(),
                    attribute: "FileType",
                    value: "x".into(),
                },
                "unsupported FileType value \"x\" on ODM",
            ),
            (
                Error::MetadataConflict {
                    kind: Definition::Item,
                    study: "S".into(),
                    version: "M".into(),
                    oid: "I".into(),
                },
                "conflicting item definition for S/M/I",
            ),
            (
                Error::Value {
                    item: "I".into(),
                    refusal: ValueRefusal::NullWithValue,
                },
                "ItemData \"I\" is null and contains a value",
            ),
            (
                Error::Transactional {
                    element: "ODM".into(),
                    attribute: "FileType",
                    value: "Transactional".into(),
                },
                "ODM declares FileType Transactional, so the document is not a snapshot",
            ),
            (Error::NoClinicalData, "the document has no ClinicalData"),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
        assert_eq!(
            Error::MetadataConflict {
                kind: Definition::Group,
                study: "S".into(),
                version: "M".into(),
                oid: "G".into(),
            }
            .to_string(),
            "conflicting group definition for S/M/G"
        );
        assert_eq!(
            Error::Document(Document::Namespace { namespace: None }).to_string(),
            "unsupported ODM namespace (none)"
        );
        assert_eq!(
            Error::Value {
                item: "I.TEST".into(),
                refusal: ValueRefusal::ChildElements,
            }
            .to_string(),
            "ItemData \"I.TEST\" Value contains child elements"
        );
    }
}
