//! The refusals of an ODM source (ODM-1) project into the issues they report:
//! a `source_odm_*` condition in the `ingest` phase naming the source as
//! given in `path`, with no spec path for `read_odm`, and an input's dataset
//! and `input.<dataset>.path` when an input reads the source (#3030 D73,
//! D158). An option below its minimum is `value_not_permitted` at its name,
//! and a host read failure the opaque resource refusal.
use super::rendered;
use std::collections::{BTreeMap, BTreeSet};
use yamaa_core::diagnostic::{ConditionCode, OdmReadFinding};
use yamaa_core::json::Value as V;
use yamaa_core::odm_items::{Definition, Document, Error, Layout, Limit, Member, ValueRefusal};

fn text(value: &str) -> V {
    V::String(value.into())
}

/// One refusal, the condition it reports, the facts beside `path`, and its
/// rendered message.
type Case = (Error, &'static str, Vec<(&'static str, V)>, &'static str);

fn cases() -> Vec<Case> {
    vec![
        (
            Error::Archive {
                reason: "truncated TAR header",
            },
            "source_odm_archive_invalid",
            vec![("reason", text("truncated TAR header"))],
            "the archive edc/odm.xml cannot be read: truncated TAR header",
        ),
        (
            Error::Compression { format: "xz" },
            "source_odm_compression_unsupported",
            vec![("format", text("xz"))],
            "edc/odm.xml uses xz compression, but only plain and gzip TAR archives are read",
        ),
        (
            Error::Member(Member::Candidates { count: 2 }),
            "source_odm_archive_member",
            vec![(
                "reason",
                text("TAR input must contain exactly one XML member or receive archive_member"),
            )],
            "no member of edc/odm.xml can be read: TAR input must contain exactly one XML \
             member or receive archive_member",
        ),
        (
            Error::Member(Member::Matches {
                name: "study/odm.xml".into(),
                count: 0,
            }),
            "source_odm_archive_member",
            vec![(
                "reason",
                text("TAR input contains 0 entries named \"study/odm.xml\""),
            )],
            "no member of edc/odm.xml can be read: TAR input contains 0 entries named \
             \"study/odm.xml\"",
        ),
        (
            Error::Limit(Limit::Depth(64)),
            "source_odm_limit_exceeded",
            vec![("reason", text("XML depth exceeds 64"))],
            "edc/odm.xml exceeds a read limit: XML depth exceeds 64",
        ),
        (
            Error::Malformed {
                position: 41,
                reason: "unclosed element".into(),
            },
            "source_odm_xml_malformed",
            vec![
                ("position", V::from(41_u64)),
                ("reason", text("unclosed element")),
            ],
            "edc/odm.xml is not well-formed XML at byte 41: unclosed element",
        ),
        (
            Error::Doctype,
            "source_odm_xml_unsupported",
            vec![("reason", text("DOCTYPE declarations are not supported"))],
            "edc/odm.xml uses XML the reader does not support: DOCTYPE declarations are not \
             supported",
        ),
        (
            Error::Encoding {
                encoding: "UTF-16".into(),
            },
            "source_odm_xml_unsupported",
            vec![(
                "reason",
                text("ODM XML must be encoded as UTF-8, not UTF-16"),
            )],
            "edc/odm.xml uses XML the reader does not support: ODM XML must be encoded as \
             UTF-8, not UTF-16",
        ),
        (
            Error::Document(Document::Namespace { namespace: None }),
            "source_odm_document_unsupported",
            vec![("reason", text("unsupported ODM namespace (none)"))],
            "edc/odm.xml is not a supported ODM 1.3 or 2.0 document: unsupported ODM \
             namespace (none)",
        ),
        (
            Error::Layout(Layout::Nested("FormData")),
            "source_odm_layout_unsupported",
            vec![("reason", text("nested FormData is not supported"))],
            "edc/odm.xml holds clinical data the item table cannot represent: nested FormData \
             is not supported",
        ),
        (
            Error::MissingAttribute {
                element: "ItemData",
                attribute: "ItemOID",
            },
            "source_odm_attribute_missing",
            vec![
                ("element", text("ItemData")),
                ("attribute", text("ItemOID")),
            ],
            "ItemData in edc/odm.xml requires a non-empty ItemOID",
        ),
        (
            Error::AmbiguousAttribute {
                name: "StudySubjectID",
            },
            "source_odm_attribute_ambiguous",
            vec![("attribute", text("StudySubjectID"))],
            "edc/odm.xml has more than one namespaced attribute named StudySubjectID",
        ),
        (
            Error::InvalidAttribute {
                element: "ItemData".into(),
                attribute: "IsNull",
                value: "maybe".into(),
            },
            "source_odm_attribute_invalid",
            vec![
                ("element", text("ItemData")),
                ("attribute", text("IsNull")),
                ("value", text("maybe")),
            ],
            "ItemData in edc/odm.xml has the unsupported IsNull value 'maybe'",
        ),
        (
            Error::InvalidAttribute {
                element: "ODM".into(),
                attribute: "FileType",
                value: "Archive".into(),
            },
            "source_odm_attribute_invalid",
            vec![
                ("element", text("ODM")),
                ("attribute", text("FileType")),
                ("value", text("Archive")),
            ],
            "ODM in edc/odm.xml has the unsupported FileType value 'Archive'",
        ),
        (
            Error::MetadataConflict {
                kind: Definition::Item,
                study: "S".into(),
                version: "M".into(),
                oid: "I.AGE".into(),
            },
            "source_odm_metadata_conflict",
            vec![
                ("definition", text("item")),
                ("study", text("S")),
                ("metadata_version", text("M")),
                ("oid", text("I.AGE")),
            ],
            "edc/odm.xml gives the item I.AGE of S/M different names",
        ),
        (
            Error::Value {
                item: "I.AGE".into(),
                refusal: ValueRefusal::MultipleChildren,
            },
            "source_odm_value_unsupported",
            vec![
                ("item", text("I.AGE")),
                ("reason", text("has multiple Value children")),
            ],
            "the ItemData I.AGE in edc/odm.xml holds a value the item table cannot represent: \
             has multiple Value children",
        ),
        (
            Error::Transactional {
                element: "ODM".into(),
                attribute: "FileType",
                value: "Transactional".into(),
            },
            "source_odm_transactional",
            vec![
                ("element", text("ODM")),
                ("attribute", text("FileType")),
                ("value", text("Transactional")),
            ],
            "edc/odm.xml is not a snapshot: ODM declares FileType Transactional",
        ),
        (
            Error::Transactional {
                element: "ItemGroupData".into(),
                attribute: "TransactionType",
                value: "Remove".into(),
            },
            "source_odm_transactional",
            vec![
                ("element", text("ItemGroupData")),
                ("attribute", text("TransactionType")),
                ("value", text("Remove")),
            ],
            "edc/odm.xml is not a snapshot: ItemGroupData declares TransactionType Remove",
        ),
        (
            Error::NoClinicalData,
            "source_odm_no_clinical_data",
            vec![],
            "edc/odm.xml holds no ClinicalData",
        ),
    ]
}

pub(crate) fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    for (refusal, condition, facts, message) in cases() {
        let finding = OdmReadFinding {
            refusal: &refusal,
            path: "edc/odm.xml",
            dataset: None,
        };
        let diagnostic = finding.diagnostic();
        let definition = diagnostic.definition();
        assert_eq!(
            (definition.phase, definition.condition, definition.rule),
            ("ingest", condition, Some("ODM-1")),
            "{refusal:?}"
        );
        assert_eq!(finding.code(), Some(diagnostic.code));
        assert!(diagnostic.spec_paths.is_empty(), "{refusal:?}");
        let mut context = BTreeMap::from([("path".to_owned(), text("edc/odm.xml"))]);
        context.extend(
            facts
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value)),
        );
        assert_eq!(diagnostic.context, context, "{refusal:?}");
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
        assert_eq!(diagnostic.message(), message);
        reached.insert(rendered(&diagnostic));
    }

    // An input that reads the source names its dataset and declaring path.
    let removed = Error::Transactional {
        element: "ItemData".into(),
        attribute: "TransactionType",
        value: "Remove".into(),
    };
    let input = OdmReadFinding {
        refusal: &removed,
        path: "raw/odm.xml",
        dataset: Some("RAW"),
    }
    .diagnostic();
    assert_eq!(input.condition(), "source_odm_transactional");
    assert_eq!(input.spec_paths, ["input.RAW.path"]);
    assert_eq!(input.context["dataset"], text("RAW"));
    assert_eq!(input.context["path"], text("raw/odm.xml"));
    reached.insert(rendered(&input));

    // An option below its minimum is refused at its name, before the source
    // is read, as a submission's `created` argument is.
    let option = Error::Option {
        option: "max_xml_depth",
        minimum: 8,
    };
    let diagnostic = OdmReadFinding {
        refusal: &option,
        path: "edc/odm.xml",
        dataset: None,
    }
    .diagnostic();
    let definition = diagnostic.definition();
    assert_eq!(
        (definition.phase, definition.condition, definition.rule),
        ("validation", "value_not_permitted", Some("ODM-1"))
    );
    assert_eq!(diagnostic.spec_paths, ["max_xml_depth"]);
    assert_eq!(
        diagnostic.context,
        BTreeMap::from([("minimum".to_owned(), V::from(8_u64))])
    );
    assert_eq!(
        diagnostic.message(),
        "max_xml_depth has a value that is not permitted"
    );
    reached.insert(rendered(&diagnostic));

    // A source the host cannot read is no fact about the document: the
    // opaque resource refusal, which cites no rule.
    let unreadable = Error::Io {
        reason: "Input/output error".into(),
    };
    let finding = OdmReadFinding {
        refusal: &unreadable,
        path: "edc/odm.xml",
        dataset: None,
    };
    assert_eq!(finding.code(), None);
    let diagnostic = finding.diagnostic();
    assert_eq!(
        (diagnostic.condition(), diagnostic.rule()),
        ("engine_rejected", None)
    );
    assert_eq!(
        diagnostic.context,
        BTreeMap::from([
            ("step".to_owned(), text("ingest")),
            ("code".to_owned(), text("resource_unreadable")),
        ])
    );
    assert!(diagnostic.spec_paths.is_empty());
    reached.insert(rendered(&diagnostic));
    reached
}
