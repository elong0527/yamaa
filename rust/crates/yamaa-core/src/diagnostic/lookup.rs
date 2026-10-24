//! One cause per lookup fault, whichever kind of lookup raises it (LOOKUP-25):
//! an implicit join, a named intermediate or an aggregate. Each finding keeps
//! the facts of the site that raised it.
use super::{ConditionCode as C, Context, Diagnostic};
use crate::json::Value as JsonValue;
use crate::result_type::ResultType;

fn text(value: &str) -> JsonValue {
    JsonValue::String(value.into())
}

fn texts(values: &[&str]) -> JsonValue {
    JsonValue::Array(values.iter().map(|value| text(value)).collect())
}

fn finding(code: C, paths: Vec<String>, context: Context) -> Diagnostic {
    Diagnostic {
        code,
        spec_paths: paths,
        context,
        source_span: None,
        operand_route: None,
    }
}

/// The lookup that raised a finding, named in its context when it has a
/// name: a named intermediate, or the variable a source binding reads.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Owner<'a> {
    Intermediate(&'a str),
    Source(&'a str),
    Unnamed,
}

impl Owner<'_> {
    pub(crate) fn add(self, context: &mut Context) {
        match self {
            Self::Intermediate(name) => {
                context.insert("intermediate".into(), text(name));
            }
            Self::Source(variable) => {
                context.insert("variable".into(), text(variable));
            }
            Self::Unnamed => {}
        }
    }
}

/// A key pair whose two sides cannot compare (LOOKUP-9), from static
/// admission and from a raw runtime comparison alike.
pub(crate) fn key_type(
    path: &str,
    owner: Owner<'_>,
    source: &str,
    expected: ResultType,
    actual: ResultType,
) -> Diagnostic {
    let mut context: Context = [
        ("source".into(), text(source)),
        ("expected".into(), text(&expected.name())),
        ("actual".into(), text(&actual.name())),
    ]
    .into();
    owner.add(&mut context);
    finding(C::LookupKeyType, vec![path.into()], context)
}

/// A read of an input that declares no `key`, by an implicit join or by a
/// qualified aggregate without `key` of its own, at the read (LOOKUP-1,
/// LOOKUP-5; D23, D63).
pub(crate) fn undeclared_input_key(path: &str, dataset: &str, aggregate: bool) -> Diagnostic {
    let hint = if aggregate {
        "declare `key` on the input or on the aggregate"
    } else {
        "declare `key` on the input"
    };
    finding(
        C::LookupNoApplicableKeys,
        vec![path.into()],
        [
            ("dataset".into(), text(dataset)),
            ("hint".into(), text(hint)),
        ]
        .into(),
    )
}

/// An intermediate read as a lookup without `key`, at its `key` (LOOKUP-3,
/// ROW-2; D23, D63). The record is the schema's own for a missing required
/// field: the field and its class.
pub(crate) fn key_required(path: &str) -> Diagnostic {
    finding(
        C::IntermediateKeyRequired,
        vec![path.into()],
        [
            ("field".into(), text("key")),
            ("class".into(), text("intermediate_class")),
        ]
        .into(),
    )
}

/// A written `key` that names no pair (LOOKUP-5).
pub(crate) fn empty_key(path: &str, owner: Owner<'_>, dataset: &str) -> Diagnostic {
    let mut context: Context = [
        ("dataset".into(), text(dataset)),
        ("expected".into(), text("at least one match pair")),
    ]
    .into();
    owner.add(&mut context);
    finding(C::LookupEmptyKey, vec![path.into()], context)
}

/// A `key` column the relation does not have (LOOKUP-5).
pub(crate) fn donor_key_reference(path: &str, owner: Owner<'_>, identifier: &str) -> Diagnostic {
    let mut context: Context = [("identifier".into(), text(identifier))].into();
    owner.add(&mut context);
    finding(C::LookupDonorKeyReference, vec![path.into()], context)
}

/// A match value that reads no known current-row value (LOOKUP-5).
pub(crate) fn match_value_reference(path: &str, owner: Owner<'_>, identifier: &str) -> Diagnostic {
    let mut context: Context = [("identifier".into(), text(identifier))].into();
    owner.add(&mut context);
    finding(C::LookupMatchValueReference, vec![path.into()], context)
}

/// A `between` that states neither bound (LOOKUP-9).
pub(crate) fn range_declaration(path: &str, owner: Owner<'_>) -> Diagnostic {
    let mut context: Context = [("expected".into(), text("at least one of lower, upper"))].into();
    owner.add(&mut context);
    finding(C::LookupRangeDeclaration, vec![path.into()], context)
}

/// A `between` bound or value that names no admitted field (LOOKUP-9).
pub(crate) fn range_reference(path: &str, owner: Owner<'_>, identifier: &str) -> Diagnostic {
    let mut context: Context = [("identifier".into(), text(identifier))].into();
    owner.add(&mut context);
    finding(C::LookupRangeReference, vec![path.into()], context)
}

/// `order_by` without `keep`, or `keep` without `order_by`; or an
/// aggregate's reducer field beside an expression that does not read it,
/// or `STR_JOIN` without the fields it needs (LOOKUP-10).
pub(crate) fn selection_unpaired(
    path: &str,
    owner: Owner<'_>,
    declared: &[&str],
    missing: &[&str],
) -> Diagnostic {
    let mut context: Context = [
        ("declared".into(), texts(declared)),
        ("missing".into(), texts(missing)),
    ]
    .into();
    match owner {
        // A source binding names the operation it belongs to.
        Owner::Source(_) => {
            context.insert("operation".into(), text("source"));
        }
        owner => owner.add(&mut context),
    }
    finding(C::LookupSelectionUnpaired, vec![path.into()], context)
}

/// `order_by` or `keep` beside a nearest selection, which replaces them
/// (LOOKUP-26): each field written, then `nearest`, in specification order.
/// `construct` is the first field written, which its message names.
pub(crate) fn nearest_beside_selection(
    paths: Vec<String>,
    owner: Owner<'_>,
    construct: &str,
) -> Diagnostic {
    let mut context: Context = [("construct".into(), text(construct))].into();
    owner.add(&mut context);
    finding(C::LookupNearestSelection, paths, context)
}

/// A nearest selection's `source` that names no field of the lookup's
/// relation (LOOKUP-26).
pub(crate) fn nearest_source_reference(
    path: &str,
    owner: Owner<'_>,
    identifier: &str,
) -> Diagnostic {
    let mut context: Context = [("identifier".into(), text(identifier))].into();
    owner.add(&mut context);
    finding(C::LookupNearestSourceReference, vec![path.into()], context)
}

/// A nearest selection's `source` whose type is neither `date` nor
/// `datetime` (LOOKUP-26).
pub(crate) fn nearest_source_type(
    path: &str,
    owner: Owner<'_>,
    identifier: &str,
    actual: ResultType,
) -> Diagnostic {
    let mut context: Context = [
        ("identifier".into(), text(identifier)),
        ("actual".into(), text(&actual.name())),
        ("permitted".into(), texts(&["date", "datetime"])),
    ]
    .into();
    owner.add(&mut context);
    finding(C::LookupNearestSourceType, vec![path.into()], context)
}

/// A nearest selection's `to` whose type is not its `source`'s, before any
/// record is read or when a value decided per value is read (LOOKUP-26).
pub(crate) fn nearest_reference_type(
    path: &str,
    owner: Owner<'_>,
    expected: ResultType,
    actual: ResultType,
) -> Diagnostic {
    let mut context: Context = [
        ("expected".into(), text(&expected.name())),
        ("actual".into(), text(&actual.name())),
    ]
    .into();
    owner.add(&mut context);
    finding(C::LookupNearestReferenceType, vec![path.into()], context)
}

/// `require_match: true` beside a declared absence value (LOOKUP-24), naming
/// the dataset the lookup reads.
pub(crate) fn absence_conflict(
    answer: String,
    require_match: String,
    owner: Owner<'_>,
    dataset: &str,
) -> Diagnostic {
    let mut context: Context = [("dataset".into(), text(dataset))].into();
    owner.add(&mut context);
    finding(
        C::LookupAbsenceConflict,
        vec![answer, require_match],
        context,
    )
}
