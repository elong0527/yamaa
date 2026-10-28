//! One cause per lookup fault, whichever kind of lookup raises it (LOOKUP-25):
//! an implicit join, a named intermediate or an aggregate. Each finding keeps
//! the facts of the site that raised it.
use super::{ConditionCode as C, Context, Diagnostic};
use crate::expression_table::Verb;
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
/// name: a named intermediate, or the variable a source binding reads; or
/// the window operation whose order a finding names.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Owner<'a> {
    Intermediate(&'a str),
    Source(&'a str),
    Window(&'a str),
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
            Self::Window(operation) => {
                context.insert("operation".into(), text(operation));
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
/// LOOKUP-5; D23, D63). A pair relation, which can declare no input `key`,
/// is read through a lookup or an aggregate keyed on its own (LOOKUP-27).
pub(crate) fn undeclared_input_key(
    path: &str,
    dataset: &str,
    aggregate: bool,
    pair: bool,
) -> Diagnostic {
    let hint = match (pair, aggregate) {
        (true, _) => "read it through a named intermediate or an aggregate with a `key` of its own",
        (false, true) => "declare `key` on the input or on the aggregate",
        (false, false) => "declare `key` on the input",
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
            context.insert("operation".into(), text(Verb::Source.name()));
        }
        owner => owner.add(&mut context),
    }
    finding(C::LookupSelectionUnpaired, vec![path.into()], context)
}

/// An order term whose field holds values of types that do not all compare
/// under VALUE-19, a type with itself included, so the term orders nothing
/// (VALUE-22): a named intermediate's `order_by` or a window's order, before
/// any record is read. `actual` is the field's type, decided per value.
pub(crate) fn order_term_type(
    path: &str,
    owner: Owner<'_>,
    identifier: &str,
    actual: ResultType,
) -> Diagnostic {
    let mut context: Context = [
        ("identifier".into(), text(identifier)),
        ("actual".into(), text(&actual.name())),
    ]
    .into();
    owner.add(&mut context);
    finding(C::OrderTermType, vec![path.into()], context)
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

/// What a pair relation's `inner_join` admits none of beside it (LOOKUP-27):
/// each field written that selects, ranges or answers absence, and a side
/// that names `SELF`, then `inner_join`, in LOOKUP-27's listed order. `construct`
/// is the first of them, a field's name or `SELF`, which its message names.
pub(crate) fn pair_construct(
    paths: Vec<String>,
    intermediate: &str,
    construct: &str,
) -> Diagnostic {
    finding(
        C::LookupPairConstruct,
        paths,
        [
            ("intermediate".into(), text(intermediate)),
            ("construct".into(), text(construct)),
        ]
        .into(),
    )
}

/// A pair relation without its `key` or its `derivations`, each of which it
/// requires (LOOKUP-27). The record is the schema's own for a missing
/// required field: the field and its class.
pub(crate) fn pair_required(path: &str, field: &str) -> Diagnostic {
    finding(
        if field == "key" {
            C::LookupPairKeyRequired
        } else {
            C::LookupPairDerivationsRequired
        },
        vec![path.into()],
        [
            ("field".into(), text(field)),
            ("class".into(), text("intermediate_class")),
        ]
        .into(),
    )
}

/// A side of a pair relation that its records cannot come from, at the
/// pair's field that names it (LOOKUP-27): an intermediate that declares a
/// field a row driver may not, or that reads no input (ROW-2), named with
/// each such field.
pub(crate) fn pair_side(path: &str, side: &str, fields: &[&str]) -> Diagnostic {
    finding(
        C::LookupPairSide,
        vec![path.into()],
        [
            ("intermediate".into(), text(side)),
            ("fields".into(), texts(fields)),
        ]
        .into(),
    )
}

/// A pair relation whose two sides are one relation, which no qualifier
/// could tell apart (LOOKUP-27): at `inner_join`, then `dataset`.
pub(crate) fn pair_side_repeated(
    inner_join: String,
    dataset: String,
    identifier: &str,
) -> Diagnostic {
    finding(
        C::LookupPairSideRepeated,
        vec![inner_join, dataset],
        [("identifier".into(), text(identifier))].into(),
    )
}

/// Fields without one column type (EXPR-14), at the pair relation's field
/// that exposes them (LOOKUP-27): a side intermediate's derived fields at
/// the side's field, or the pair relation's own derivations.
pub(crate) fn pair_field_type(path: &str, intermediate: &str, columns: &[&str]) -> Diagnostic {
    finding(
        C::LookupPairFieldType,
        vec![path.into()],
        [
            ("intermediate".into(), text(intermediate)),
            ("columns".into(), texts(columns)),
        ]
        .into(),
    )
}

/// A pair relation's key column, or the left value of a mapping pair, that
/// names no field of its side, at `key` (LOOKUP-27). `identifier` is the
/// name qualified by the side.
pub(crate) fn pair_key_reference(path: &str, intermediate: &str, identifier: &str) -> Diagnostic {
    finding(
        C::LookupPairKeyReference,
        vec![path.into()],
        [
            ("intermediate".into(), text(intermediate)),
            ("identifier".into(), text(identifier)),
        ]
        .into(),
    )
}

/// A pair relation's key pair whose two sides' types cannot compare
/// (LOOKUP-27, LOOKUP-9): `source` is the right side's column qualified by
/// it, `expected` the left value's type and `actual` the column's.
pub(crate) fn pair_key_type(
    path: &str,
    intermediate: &str,
    source: &str,
    expected: ResultType,
    actual: ResultType,
) -> Diagnostic {
    finding(
        C::LookupPairKeyType,
        vec![path.into()],
        [
            ("intermediate".into(), text(intermediate)),
            ("source".into(), text(source)),
            ("expected".into(), text(&expected.name())),
            ("actual".into(), text(&actual.name())),
        ]
        .into(),
    )
}

/// A name a pair relation's `filter` or derivation reads that is no field of
/// its two sides, qualified by the side, and no derivation declared before
/// it (LOOKUP-27).
pub(crate) fn pair_reference(path: &str, intermediate: &str, identifier: &str) -> Diagnostic {
    finding(
        C::LookupPairReference,
        vec![path.into()],
        [
            ("intermediate".into(), text(intermediate)),
            ("identifier".into(), text(identifier)),
        ]
        .into(),
    )
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
