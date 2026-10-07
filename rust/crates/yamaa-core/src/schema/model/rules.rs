//! Normalized model shape, independent of the authored schema's shorthand rules.
use super::{Field, Kind, Model};
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
        optional!("parents", STRINGS),
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
        optional!("types", Dictionary(&COLUMN_TYPE)),
        optional!("schema", Text),
        field!("empty_string", Enum(&["missing", "present"])),
        optional!("ordinal", Ordinal),
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
        optional!("decimals", Integer),
        required!("columns", STRINGS),
        optional!("warning_log", Text),
        optional!("verification_log", Text),
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
    [required!(
        "unique",
        Union(&[
            ("list[str]", STRINGS),
            (
                "function-after[reject_null_id(), IntermediateUnique]",
                Object(UNIQUE)
            )
        ])
    ),]
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
        optional!("derivations", HANDLED_MAP),
        optional!("verifications", List(&Object(INTERMEDIATE_CHECK))),
        field!("no_match", Json),
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
        optional!("submission", Dictionary(&Object(SUBMISSION_COLUMN))),
    ]
);
model!(
    PAGE,
    "PageReference",
    [
        required!("type", Enum(&["PhysicalRef", "NamedDestination"])),
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
        field!("type", Enum(&["Computation", "Imputation"])),
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
        required!(
            "type",
            Enum(&[
                "Assigned",
                "Collected",
                "Derived",
                "Not Available",
                "Other",
                "Predecessor",
                "Protocol"
            ])
        ),
        optional!(
            "source",
            Enum(&["Investigator", "Sponsor", "Subject", "Vendor"])
        ),
        optional!("description", Text),
        optional!("documents", List(&DOCUMENT_REFERENCE)),
    ]
);
model!(
    SUBMISSION_COLUMN,
    "SubmissionColumn",
    [
        optional!("core", Enum(&["Req", "Exp", "Perm", "Cond"])),
        optional!("mandatory", Boolean),
        optional!("role", Text),
        optional!(
            "data_type",
            Enum(&[
                "text",
                "integer",
                "float",
                "date",
                "datetime",
                "time",
                "partialDate",
                "partialTime",
                "partialDatetime",
                "incompleteDate",
                "incompleteTime",
                "incompleteDatetime",
                "durationDatetime",
                "intervalDatetime",
                "URI"
            ])
        ),
        optional!("length", Integer),
        optional!("significant_digits", Integer),
        optional!("display_format", Text),
        optional!("codelist", Text),
        optional!("inventory_vocabulary", Boolean),
        optional!("origin", Object(ORIGIN)),
        optional!("method", METHOD_VALUE),
        optional!("comment", COMMENT_VALUE),
    ]
);
model!(
    SUBMISSION_DATASET,
    "SubmissionDataset",
    [
        required!("label", Text),
        required!(
            "class",
            Enum(&[
                "ADAM OTHER",
                "BASIC DATA STRUCTURE",
                "DEVICE LEVEL ANALYSIS DATASET",
                "EVENTS",
                "FINDINGS",
                "FINDINGS ABOUT",
                "INTERVENTIONS",
                "MEDICAL DEVICE BASIC DATA STRUCTURE",
                "MEDICAL DEVICE OCCURRENCE DATA STRUCTURE",
                "OCCURRENCE DATA STRUCTURE",
                "REFERENCE DATA STRUCTURE",
                "RELATIONSHIP",
                "SPECIAL PURPOSE",
                "STUDY REFERENCE",
                "SUBJECT LEVEL ANALYSIS DATASET",
                "TRIAL DESIGN"
            ])
        ),
        optional!(
            "subclass",
            Enum(&[
                "ADVERSE EVENT",
                "MEDICAL DEVICE TIME-TO-EVENT",
                "NON-COMPARTMENTAL ANALYSIS",
                "POPULATION PHARMACOKINETIC ANALYSIS",
                "TIME-TO-EVENT"
            ])
        ),
        required!("structure", Text),
        required!("repeating", Boolean),
        field!("reference_data", Boolean),
        optional!("domain", Text),
        optional!("comment", COMMENT_VALUE),
    ]
);
