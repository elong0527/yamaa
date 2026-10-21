//! Normalized model shape, independent of the authored schema's shorthand rules.
use super::{Field, Kind, Model};
use crate::specification_submission_document::{
    Core, DataType, DatasetClass, DatasetSubclass, MethodType, OriginSource, OriginType, PageType,
    ROLES,
};
use Kind::*;

macro_rules! required {
    ($name:literal, $kind:expr) => {
        Field {
            name: $name,
            kind: $kind,
            required: true,
        }
    };
}
macro_rules! field {
    ($name:literal, $kind:expr) => {
        Field {
            name: $name,
            kind: $kind,
            required: false,
        }
    };
}
macro_rules! optional {
    ($name:literal, $kind:expr) => {
        field!($name, Optional(&$kind))
    };
}
macro_rules! model {
    ($constant:ident, $name:literal, [$($field:expr),* $(,)?]) => {
        pub(super) const $constant: Model = Model { name: $name, fields: &[$($field),*] };
    };
}
const COLUMN_TYPE: Kind = Enum(&["str", "int", "float", "date", "datetime"]);
const STRINGS: Kind = List(&Text);
const ORDER_TERMS: Kind = List(&Object(ORDER));
const HANDLED_MAP: Kind = Dictionary(&Object(HANDLED));
const DOCUMENT_REFERENCE: Kind =
    Union(&[("str", Text), ("DocumentReferenceClass", Object(DOCUMENT))]);
const METHOD_VALUE: Kind = Union(&[("str", Text), ("SubmissionMethodClass", Object(METHOD))]);
const COMMENT_VALUE: Kind = Union(&[("str", Text), ("SubmissionCommentClass", Object(COMMENT))]);

model!(
    SPECIFICATION,
    "Specification",
    [
        required!("schema_version", Text),
        required!("domain", Text),
        required!("input", Dictionary(&Object(SOURCE))),
        optional!("base", Text),
        optional!("parent", Text),
        optional!("intermediates", List(&Object(INTERMEDIATE))),
        required!("keys", STRINGS),
        required!("output", Object(OUTPUT)),
        required!("columns", List(&Object(COLUMN))),
        optional!("rows", List(&Object(ROW))),
        optional!("filter", Text),
        optional!("verifications", List(&Expression)),
        optional!("submission", Object(SUBMISSION_DATASET)),
        optional!("metadata", Dictionary(&Text)),
    ]
);
model!(
    SOURCE,
    "DatasetSource",
    [
        required!("path", Text),
        field!("relative_to", Enum(&["declaring", "entry"])),
        optional!("types", Dictionary(&COLUMN_TYPE)),
        optional!(
            "encoding",
            Enum(&["utf-8", "windows-1252", "iso-8859-1", "ascii"])
        ),
        optional!("schema", Text),
        field!("empty_string", Enum(&["missing", "present"])),
        optional!("ordinal", Ordinal),
        optional!("require_match", Boolean),
    ]
);
model!(
    ORDER,
    "OrderTerm",
    [
        required!("variable", Text),
        field!("direction", Enum(&["asc", "desc"])),
        field!("nulls", Enum(&["first", "last"])),
    ]
);
model!(
    OUTPUT,
    "Output",
    [
        required!("path", Text),
        field!("relative_to", Enum(&["declaring", "entry"])),
        optional!("decimals", Integer),
        required!("columns", STRINGS),
        optional!("report", Text),
        optional!("order_by", ORDER_TERMS),
    ]
);
model!(
    BETWEEN,
    "RecordBetween",
    [
        required!("value", Text),
        optional!("lower", Text),
        optional!("upper", Text)
    ]
);
model!(
    UNIQUE,
    "IntermediateUnique",
    [
        required!("columns", NonemptyStrings),
        optional!("id", NonemptyText),
    ]
);
model!(
    INTERMEDIATE_CHECK,
    "IntermediateVerification",
    [required!("unique", Object(UNIQUE))]
);
model!(
    INTERMEDIATE,
    "Intermediate",
    [
        required!("id", Text),
        required!("dataset", Text),
        optional!(
            "key",
            Union(&[
                ("list[str]", STRINGS),
                (
                    "dict[str,union[str,function-after[require_one_operation(), Expression]]]",
                    Dictionary(&Union(&[
                        ("str", Text),
                        (
                            "function-after[require_one_operation(), Expression]",
                            Expression
                        )
                    ]))
                )
            ])
        ),
        optional!("between", Object(BETWEEN)),
        optional!("filter", Text),
        optional!("order_by", ORDER_TERMS),
        optional!("keep", Enum(&["first", "last"])),
        optional!("columns", STRINGS),
        optional!(
            "derivations",
            Dictionary(&Union(&[
                ("IntermediateExpression", Object(INTERMEDIATE_EXPRESSION)),
                ("IntermediateBinding", Object(INTERMEDIATE_BINDING)),
            ]))
        ),
        optional!("verifications", List(&Object(INTERMEDIATE_CHECK))),
        field!("no_match", Json),
        optional!("require_match", Boolean),
    ]
);
model!(
    INTERMEDIATE_EXPRESSION,
    "IntermediateExpression",
    [required!("value", Expression)]
);
model!(
    INTERMEDIATE_BINDING,
    "IntermediateBinding",
    [
        required!("type", COLUMN_TYPE),
        required!("derivation", Object(INTERMEDIATE_EXPRESSION)),
        field!("unconvertible", Json)
    ]
);
model!(
    HANDLED,
    "HandledExpression",
    [
        required!("value", Expression),
        field!("unconvertible", Json)
    ]
);
model!(
    COLUMN,
    "Column",
    [
        required!("name", Text),
        required!("type", COLUMN_TYPE),
        optional!("label", Text),
        optional!("derivation", Object(HANDLED)),
        optional!("verifications", List(&Expression)),
        optional!("submission", Object(SUBMISSION_COLUMN)),
        optional!("metadata", Dictionary(&Text)),
    ]
);
model!(
    ROW,
    "Row",
    [
        required!("id", Text),
        optional!("dataset", Text),
        optional!("group_by", STRINGS),
        optional!("filter", Text),
        required!("derivations", HANDLED_MAP),
    ]
);
model!(
    PAGE,
    "PageReference",
    [
        required!("type", Enum(PageType::NAMES)),
        required!("refs", Text),
        optional!("title", Text),
    ]
);
model!(
    DOCUMENT,
    "DocumentReferenceClass",
    [
        required!("document", Text),
        optional!("pages", Object(PAGE))
    ]
);
model!(
    FORMAL,
    "FormalExpression",
    [required!("context", Text), required!("code", Text)]
);
model!(
    METHOD,
    "SubmissionMethodClass",
    [
        optional!("name", Text),
        field!("type", Enum(MethodType::NAMES)),
        required!("description", Text),
        optional!("expression", Object(FORMAL)),
        optional!("documents", List(&DOCUMENT_REFERENCE)),
    ]
);
model!(
    COMMENT,
    "SubmissionCommentClass",
    [
        required!("text", Text),
        optional!("documents", List(&DOCUMENT_REFERENCE))
    ]
);
model!(
    ORIGIN,
    "SubmissionOrigin",
    [
        required!("type", Enum(OriginType::NAMES)),
        optional!("source", Enum(OriginSource::NAMES)),
        optional!("description", Text),
        optional!("documents", List(&DOCUMENT_REFERENCE)),
    ]
);
/// The metadata fields a column and each of its value-level entries share.
macro_rules! submission_metadata {
    ($($field:expr),* $(,)?) => {
        [
            optional!("core", Enum(Core::NAMES)),
            optional!("mandatory", Boolean),
            optional!("role", Enum(ROLES)),
            optional!("data_type", Enum(DataType::NAMES)),
            optional!("length", Integer),
            optional!("significant_digits", Integer),
            optional!("display_format", Text),
            optional!("codelist", Text),
            optional!("inventory_vocabulary", Boolean),
            optional!("origin", Object(ORIGIN)),
            optional!("method", METHOD_VALUE),
            optional!("comment", COMMENT_VALUE),
            $($field),*
        ]
    };
}
pub(super) const SUBMISSION_COLUMN: Model = Model {
    name: "SubmissionColumn",
    fields: &submission_metadata!(optional!("value_level", List(&Object(SUBMISSION_VALUE)))),
};
pub(super) const SUBMISSION_VALUE: Model = Model {
    name: "SubmissionValue",
    fields: &submission_metadata!(
        required!("id", Text),
        required!("where", Dictionary(&List(&NonemptyText))),
        optional!("label", Text),
    ),
};
model!(
    SUBMISSION_DATASET,
    "SubmissionDataset",
    [
        required!("label", Text),
        required!("class", Enum(DatasetClass::NAMES)),
        optional!("subclass", Enum(DatasetSubclass::NAMES)),
        required!("structure", Text),
        required!("repeating", Boolean),
        field!("reference_data", Boolean),
        field!("has_no_data", Boolean),
        optional!("domain", Text),
        optional!("comment", COMMENT_VALUE),
    ]
);
