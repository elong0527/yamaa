//! The diagnostic contracts: the replay of the expected issues written as
//! data under `tests/fixtures/diagnostics/` (#3691), its readers, and the
//! case form it shares with the application's replay (#3798).
pub mod form;
pub mod json;
pub mod operations;
pub mod replay;

/// Decode a quoted JSON member name with the fixture reader, while the
/// shared form reader keeps selected values as their original text.
pub fn json_string(text: &str, label: &str) -> String {
    match json::read(text, label) {
        json::Json::String(text) => text,
        _ => panic!("{label}: expected a JSON string"),
    }
}

use std::{cell::RefCell, collections::BTreeSet};
use yamaa_core::{
    diagnostic::{ConditionCode, Diagnostic},
    json::Value,
    schema::{schema_diagnostic_kind_label, SchemaContext, SchemaDiagnostic},
    submission_composition::Finding,
};

thread_local! {
    static RENDERED: RefCell<BTreeSet<ConditionCode>> = const { RefCell::new(BTreeSet::new()) };
}

/// A reached finding as the record an issue carries: its cause, spec paths
/// and the context keys its message may name.
pub trait Record {
    fn record(&self) -> Diagnostic;
}
impl Record for Diagnostic {
    fn record(&self) -> Diagnostic {
        self.clone()
    }
}
impl Record for Finding {
    /// A composition finding keeps its path and context as the record's.
    fn record(&self) -> Diagnostic {
        Diagnostic {
            code: self.code,
            spec_paths: vec![self.path.clone()],
            context: self.context.clone(),
            source_span: None,
            operand_route: None,
        }
    }
}
impl Record for SchemaDiagnostic {
    /// A schema finding's context names resolve against the pass's document
    /// and schema, then gain only the caller's provenance; a value that names
    /// a document node or a descriptor stands in as text here.
    fn record(&self) -> Diagnostic {
        let value = |context: &SchemaContext| match context {
            SchemaContext::Text(text) => Value::String(text.clone()),
            SchemaContext::Boolean(value) => Value::Bool(*value),
            SchemaContext::Count(count) => Value::from(*count),
            SchemaContext::Null => Value::Null,
            SchemaContext::TextList(values) => {
                Value::Array(values.iter().cloned().map(Value::String).collect())
            }
            SchemaContext::ValueKind(_) => Value::String(
                schema_diagnostic_kind_label(context)
                    .expect("a kind has a label")
                    .into(),
            ),
            SchemaContext::InputValue(_)
            | SchemaContext::DescriptorValues(_)
            | SchemaContext::DescriptorPattern(_)
            | SchemaContext::DescriptorMinimum(_)
            | SchemaContext::DescriptorSize(_) => Value::String("value".into()),
        };
        Diagnostic {
            code: self.code,
            spec_paths: vec![self.path.clone()],
            context: self
                .context
                .iter()
                .map(|(name, context)| ((*name).into(), value(context)))
                .collect(),
            source_span: None,
            operand_route: None,
        }
    }
}

/// Render a reached finding's message from the condition registry, failing
/// when its template names a key the record does not hold (P-MSG-1), and
/// return its cause.
pub fn rendered(finding: &impl Record) -> ConditionCode {
    let record = finding.record();
    if let Err(error) = record.checked_message() {
        panic!(
            "{:?} cannot render its message: {error}; it records {:?}",
            record.code, record.context
        );
    }
    RENDERED.with(|causes| causes.borrow_mut().insert(record.code));
    record.code
}

/// Every cause this thread rendered from a reached finding, with those the
/// replay's threads rendered for it.
pub fn rendered_causes() -> BTreeSet<ConditionCode> {
    RENDERED.with(|causes| causes.borrow().clone())
}

/// Count `causes`, which the replay's threads rendered for a replay this
/// thread runs, as rendered on this thread.
pub fn rendered_for_this_thread(causes: &BTreeSet<ConditionCode>) {
    RENDERED.with(|rendered| rendered.borrow_mut().extend(causes.iter().copied()));
}
impl Record for yamaa_core::project_diagnostics::Diagnostic<'_> {
    /// A borrowed project record owns its facts the way the adapters'
    /// projection does.
    fn record(&self) -> Diagnostic {
        use yamaa_core::project_diagnostics::Fact;
        fn fact(value: &Fact<'_>) -> Value {
            match value {
                Fact::Text(text) => Value::String((*text).into()),
                Fact::OwnedText(text) => Value::String(text.clone()),
                Fact::Scalar(value) => (*value).clone().into(),
                Fact::Boolean(value) => Value::Bool(*value),
                Fact::Integer(value) => Value::from(*value),
                Fact::Unsigned(value) => Value::from(*value),
                Fact::Bound(value) => Value::Number(value.decimal().parse().unwrap()),
                Fact::Null => Value::Null,
                Fact::Sequence(values) => Value::Array(values.iter().map(fact).collect()),
                Fact::Object(values) => Value::Object(
                    values
                        .iter()
                        .map(|(name, value)| ((*name).into(), fact(value)))
                        .collect(),
                ),
            }
        }
        Diagnostic {
            code: self.code,
            spec_paths: vec![self.spec_path.clone()],
            context: self
                .context
                .iter()
                .map(|(name, value)| ((*name).into(), fact(value)))
                .collect(),
            source_span: None,
            operand_route: None,
        }
    }
}
