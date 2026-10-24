//! The findings of a qualified or row aggregate (LOOKUP-5, LOOKUP-9,
//! LOOKUP-18, LOOKUP-19, FORMULA-20, FORMULA-22, FORMULA-23). Its derive
//! bindings' refusals are preparation's, read by the shared `prepare`.
use super::{column_type, text, value, value_type};
use crate::support::diagnostic::json::Json;
use yamaa_core::{
    dataset::AggregatePhase,
    diagnostic::{AggregateContextCause as Cause, AggregateFinding as Finding, Diagnostic},
    expression::{compile, Context},
    result_type::ResultType,
};

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "aggregate_finding" => vec![aggregate_finding(input)],
        "aggregate_types" => aggregate_types(input),
        _ => return None,
    })
}

/// A finding of an aggregate at `path`, `{finding, path, ...}`:
/// `missing_keys` and `undeclared_input_key` with `dataset`,
/// `multiple_records` with `expression`, `phase`, `count`, `row` and `group`
/// (`row` `null` outside row construction), `identifier_not_grouped` with
/// `expression`, `dataset` and `identifier`, `incomparable_sources` with
/// `expression`, `source` and `types`, `mixed_relations` with `expression`
/// and `relations`, `invalid_context` with `expression` and `cause`,
/// `donor_key_reference` and `match_value_reference` with `identifier`, and
/// `key_type` with `source`, `expected` and `actual`.
fn aggregate_finding(input: &Json) -> Diagnostic {
    let path = text(input, "path");
    let expression = || text(input, "expression");
    let optional = |key| match input.field(key) {
        Json::Null => None,
        written => Some(written.text()),
    };
    let (group, types, relations);
    let finding = match text(input, "finding") {
        "missing_keys" => {
            input.closed(&["finding", "path", "dataset"]);
            Finding::MissingKeys {
                path,
                dataset: text(input, "dataset"),
            }
        }
        "multiple_records" => {
            input.closed(&[
                "finding",
                "path",
                "expression",
                "phase",
                "count",
                "row",
                "group",
            ]);
            group = input
                .field("group")
                .members()
                .iter()
                .map(|(name, held)| (name.clone(), value(held)))
                .collect::<Vec<_>>();
            Finding::MultipleRecords {
                path,
                expression: expression(),
                phase: match text(input, "phase") {
                    "derivation" => AggregatePhase::Derivation,
                    "row_construction" => AggregatePhase::RowConstruction,
                    other => panic!("no aggregate phase {other:?}"),
                },
                count: input.field("count").integer(),
                row: optional("row"),
                group: &group,
            }
        }
        "identifier_not_grouped" => {
            input.closed(&["finding", "path", "expression", "dataset", "identifier"]);
            Finding::IdentifierNotGrouped {
                path,
                expression: expression(),
                dataset: optional("dataset"),
                identifier: text(input, "identifier"),
            }
        }
        "incomparable_sources" => {
            input.closed(&["finding", "path", "expression", "source", "types"]);
            types = input
                .field("types")
                .items()
                .iter()
                .map(value_type)
                .collect::<Vec<_>>();
            Finding::IncomparableSources {
                path,
                expression: expression(),
                source: text(input, "source"),
                types: &types,
            }
        }
        "mixed_relations" => {
            input.closed(&["finding", "path", "expression", "relations"]);
            relations = input
                .field("relations")
                .items()
                .iter()
                .map(|relation| relation.text().to_owned())
                .collect::<Vec<_>>();
            Finding::MixedRelations {
                path,
                expression: expression(),
                relations: &relations,
            }
        }
        "invalid_context" => {
            input.closed(&["finding", "path", "expression", "cause"]);
            Finding::InvalidContext {
                path,
                expression: expression(),
                cause: match text(input, "cause") {
                    "row_owns_groups" => Cause::RowOwnsGroups,
                    "row_no_right_side" => Cause::RowNoRightSide,
                    "row_no_keys" => Cause::RowNoKeys,
                    "ungrouped_row" => Cause::UngroupedRow,
                    "output_needs_groups" => Cause::OutputNeedsGroups,
                    "output_groups_qualified" => Cause::OutputGroupsQualified,
                    "output_between" => Cause::OutputBetween,
                    "qualified_group_not_output_key" => Cause::QualifiedGroupNotOutputKey,
                    other => panic!("no aggregate context cause {other:?}"),
                },
            }
        }
        "donor_key_reference" => {
            input.closed(&["finding", "path", "identifier"]);
            Finding::DonorKeyReference {
                path,
                identifier: text(input, "identifier"),
            }
        }
        "match_value_reference" => {
            input.closed(&["finding", "path", "identifier"]);
            Finding::MatchValueReference {
                path,
                identifier: text(input, "identifier"),
            }
        }
        "key_type" => {
            input.closed(&["finding", "path", "source", "expected", "actual"]);
            Finding::KeyType {
                path,
                source: text(input, "source"),
                expected: ResultType::of(value_type(input.field("expected"))),
                actual: column_type(input.field("actual")),
            }
        }
        "undeclared_input_key" => {
            input.closed(&["finding", "path", "dataset"]);
            Finding::UndeclaredInputKey {
                path,
                dataset: text(input, "dataset"),
            }
        }
        other => panic!("no aggregate finding {other:?}"),
    };
    finding.diagnostic().expect("a complete context")
}

/// The static type findings of the aggregate `expression` at `path`, before
/// any record is read, `{expression, path, types}`: `types` gives the
/// declared type of each name it lists, and no name is a collected source.
fn aggregate_types(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["expression", "path", "types"]);
    let (expression, path) = (text(input, "expression"), text(input, "path"));
    let types = input.field("types").members();
    compile(expression, Context::Aggregate, path)
        .expect("an aggregate that compiles")
        .type_findings(
            |name| {
                types
                    .iter()
                    .find(|(declared, _)| declared == name)
                    .map(|(_, kind)| value_type(kind))
            },
            false,
        )
        .iter()
        .map(|finding| {
            finding
                .diagnostic(path, expression)
                .expect("a complete context")
        })
        .collect()
}
