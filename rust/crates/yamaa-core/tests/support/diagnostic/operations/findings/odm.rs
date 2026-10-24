//! The ODM profile's findings (NAME-8, NAME-9, NAME-11, NAME-13, INPUT-4)
//! and the refusals of an ODM source (ODM-1).
use super::{column_type, text, value};
use crate::support::diagnostic::json::Json;
use yamaa_core::{
    dataset::AggregatePhase,
    diagnostic::{Diagnostic, OdmDeclared, OdmFinding, OdmLocation, OdmReadFinding},
    odm_items::{Definition, Document, Error, Layout, Limit, Member, ValueRefusal},
};

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "odm_finding" => vec![odm_finding(input)],
        "odm_read" => vec![odm_read(input)],
        _ => return None,
    })
}

/// A finding of the ODM profile, `{finding, ...}`: `declared` with `dataset`
/// and `declared`, `missing_fields` with `dataset` and `fields`,
/// `ambiguous_field` with `dataset`, `field` and `stored`, `field_type` with
/// `dataset`, `field`, `stored_field` and `kind` (`null` for a stored type
/// the item table cannot hold), `missing_values` with `dataset`, `field`,
/// `record` and `records`, `invalid_context` with `path`, `dataset` and
/// `location`, and `not_unique` with `path`, `item`, `row`, `phase`, `scope`,
/// `differ` and `records`.
fn odm_finding(input: &Json) -> Diagnostic {
    let dataset = || text(input, "dataset");
    let texts = |key| {
        input
            .field(key)
            .items()
            .iter()
            .map(Json::text)
            .collect::<Vec<_>>()
    };
    let (fields, stored, scope, differ);
    let finding = match text(input, "finding") {
        "declared" => {
            input.closed(&["finding", "dataset", "declared"]);
            OdmFinding::Declared {
                dataset: dataset(),
                declared: match text(input, "declared") {
                    "types" => OdmDeclared::Types,
                    "schema" => OdmDeclared::Schema,
                    "ordinal" => OdmDeclared::Ordinal,
                    other => panic!("no ODM input declaration {other:?}"),
                },
            }
        }
        "missing_fields" => {
            input.closed(&["finding", "dataset", "fields"]);
            fields = texts("fields");
            OdmFinding::MissingFields {
                dataset: dataset(),
                fields: &fields,
            }
        }
        "ambiguous_field" => {
            input.closed(&["finding", "dataset", "field", "stored"]);
            stored = texts("stored")
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            OdmFinding::AmbiguousField {
                dataset: dataset(),
                field: text(input, "field"),
                stored: &stored,
            }
        }
        "field_type" => {
            input.closed(&["finding", "dataset", "field", "stored_field", "kind"]);
            OdmFinding::FieldType {
                dataset: dataset(),
                field: text(input, "field"),
                stored_field: text(input, "stored_field"),
                kind: match input.field("kind") {
                    Json::Null => None,
                    kind => Some(column_type(kind)),
                },
            }
        }
        "missing_values" => {
            input.closed(&["finding", "dataset", "field", "record", "records"]);
            OdmFinding::MissingValues {
                dataset: dataset(),
                field: text(input, "field"),
                record: input.field("record").integer(),
                records: input.field("records").integer(),
            }
        }
        "invalid_context" => {
            input.closed(&["finding", "path", "dataset", "location"]);
            OdmFinding::InvalidContext {
                path: text(input, "path"),
                dataset: dataset(),
                location: match text(input, "location") {
                    "column" => OdmLocation::Column,
                    "row" => OdmLocation::Row,
                    "intermediate" => OdmLocation::Intermediate,
                    "derive" => OdmLocation::Derive,
                    other => panic!("no odm read location {other:?}"),
                },
            }
        }
        "not_unique" => {
            input.closed(&[
                "finding", "path", "item", "row", "phase", "scope", "differ", "records",
            ]);
            scope = input
                .field("scope")
                .members()
                .iter()
                .map(|(name, held)| (name.clone(), value(held)))
                .collect::<Vec<_>>();
            differ = input
                .field("differ")
                .members()
                .iter()
                .map(|(name, values)| (name.clone(), values.items().iter().map(value).collect()))
                .collect::<Vec<_>>();
            OdmFinding::NotUnique {
                path: text(input, "path"),
                item: text(input, "item"),
                row: match input.field("row") {
                    Json::Null => None,
                    row => Some(row.text()),
                },
                phase: match text(input, "phase") {
                    "derivation" => AggregatePhase::Derivation,
                    "row_construction" => AggregatePhase::RowConstruction,
                    other => panic!("no aggregate phase {other:?}"),
                },
                scope: &scope,
                differ: &differ,
                records: input.field("records").integer(),
            }
        }
        other => panic!("no ODM finding {other:?}"),
    };
    finding.diagnostic()
}

/// A refusal of the ODM source `path`, `{refusal, path, dataset}`, as
/// `read_odm` reports it, or as the input `dataset` does when the case names
/// one. `refusal` is one member naming the refusal and holding its facts:
/// `archive` (`reason`), `compression` (`format`), `member_candidates`
/// (`count`), `member_matches` (`name`, `count`), `depth_limit` (`limit`),
/// `malformed` (`position`, `reason`), `doctype`, `encoding` (`encoding`),
/// `namespace` (`namespace`, `null` for none), `nested_layout` (`element`),
/// `missing_attribute` (`element`, `attribute`), `ambiguous_attribute`
/// (`name`), `invalid_attribute` and `transactional` (`element`,
/// `attribute`, `value`), `metadata_conflict` (`definition`, `study`,
/// `version`, `oid`), `value` (`item`, `refusal`), `no_clinical_data`,
/// `option` (`option`, `minimum`, `value`) and `io` (`reason`).
fn odm_read(input: &Json) -> Diagnostic {
    let input = input.closed(&["refusal", "path", "dataset"]);
    let [(kind, facts)] = input.field("refusal").members() else {
        panic!("a refusal is one kind")
    };
    let fact = |key| text(facts, key);
    let count = |key| facts.field(key).integer();
    let refusal = match kind.as_str() {
        "archive" => {
            facts.closed(&["reason"]);
            Error::Archive {
                reason: fixed(fact("reason")),
            }
        }
        "compression" => {
            facts.closed(&["format"]);
            Error::Compression {
                format: fixed(fact("format")),
            }
        }
        "member_candidates" => {
            facts.closed(&["count"]);
            Error::Member(Member::Candidates {
                count: count("count"),
            })
        }
        "member_matches" => {
            facts.closed(&["name", "count"]);
            Error::Member(Member::Matches {
                name: fact("name").into(),
                count: count("count"),
            })
        }
        "depth_limit" => {
            facts.closed(&["limit"]);
            Error::Limit(Limit::Depth(count("limit")))
        }
        "malformed" => {
            facts.closed(&["position", "reason"]);
            Error::Malformed {
                position: facts.field("position").integer(),
                reason: fact("reason").into(),
            }
        }
        "doctype" => {
            facts.closed(&[]);
            Error::Doctype
        }
        "encoding" => {
            facts.closed(&["encoding"]);
            Error::Encoding {
                encoding: fact("encoding").into(),
            }
        }
        "namespace" => {
            facts.closed(&["namespace"]);
            Error::Document(Document::Namespace {
                namespace: match facts.field("namespace") {
                    Json::Null => None,
                    namespace => Some(namespace.text().into()),
                },
            })
        }
        "nested_layout" => {
            facts.closed(&["element"]);
            Error::Layout(Layout::Nested(fixed(fact("element"))))
        }
        "missing_attribute" => {
            facts.closed(&["element", "attribute"]);
            Error::MissingAttribute {
                element: fixed(fact("element")),
                attribute: fixed(fact("attribute")),
            }
        }
        "ambiguous_attribute" => {
            facts.closed(&["name"]);
            Error::AmbiguousAttribute {
                name: fixed(fact("name")),
            }
        }
        "invalid_attribute" => {
            facts.closed(&["element", "attribute", "value"]);
            Error::InvalidAttribute {
                element: fact("element").into(),
                attribute: fixed(fact("attribute")),
                value: fact("value").into(),
            }
        }
        "metadata_conflict" => {
            facts.closed(&["definition", "study", "version", "oid"]);
            Error::MetadataConflict {
                kind: match fact("definition") {
                    "event" => Definition::Event,
                    "form" => Definition::Form,
                    "group" => Definition::Group,
                    "item" => Definition::Item,
                    other => panic!("no ODM definition {other:?}"),
                },
                study: fact("study").into(),
                version: fact("version").into(),
                oid: fact("oid").into(),
            }
        }
        "value" => {
            facts.closed(&["item", "refusal"]);
            Error::Value {
                item: fact("item").into(),
                refusal: match fact("refusal") {
                    "foreign_child" => ValueRefusal::ForeignChild,
                    "foreign_attribute" => ValueRefusal::ForeignAttribute,
                    "text_outside" => ValueRefusal::TextOutside,
                    "attribute_and_child" => ValueRefusal::AttributeAndChild,
                    "multiple_children" => ValueRefusal::MultipleChildren,
                    "child_elements" => ValueRefusal::ChildElements,
                    "null_with_value" => ValueRefusal::NullWithValue,
                    other => panic!("no ODM value refusal {other:?}"),
                },
            }
        }
        "transactional" => {
            facts.closed(&["element", "attribute", "value"]);
            Error::Transactional {
                element: fact("element").into(),
                attribute: fixed(fact("attribute")),
                value: fact("value").into(),
            }
        }
        "no_clinical_data" => {
            facts.closed(&[]);
            Error::NoClinicalData
        }
        "option" => {
            facts.closed(&["option", "minimum", "value"]);
            Error::Option {
                option: fixed(fact("option")),
                minimum: facts.field("minimum").integer(),
                value: facts.field("value").integer(),
            }
        }
        "io" => {
            facts.closed(&["reason"]);
            Error::Io {
                reason: fact("reason").into(),
            }
        }
        other => panic!("no ODM source refusal {other:?}"),
    };
    OdmReadFinding {
        refusal: &refusal,
        path: text(input, "path"),
        dataset: input.get("dataset").map(Json::text),
    }
    .diagnostic()
}

/// A text the reader's refusals hold as `&'static str`, as a case writes
/// it; the test process keeps it.
fn fixed(text: &str) -> &'static str {
    Box::leak(text.into())
}
