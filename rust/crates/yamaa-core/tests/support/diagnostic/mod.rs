//! Native assertion helpers for independently authored diagnostic contracts.
pub mod aggregate;
pub mod binding;
pub mod copy_entry;
pub mod csv;
pub mod dataset;
pub mod declarative_flags;
pub mod front_end;
pub mod grammar;
pub mod inheritance;
pub mod intermediate_ranges;
pub mod intermediate_scopes;
pub mod language;
pub mod mapping_cut;
pub mod odm;
pub mod odm_read;
pub mod parquet;
pub mod predicate_evaluation;
pub mod preflight;
pub mod producer;
pub mod project_activation;
pub mod project_codelist_binding;
pub mod project_functions;
pub mod project_lock;
pub mod project_scope;
pub mod record_operations;
pub mod result_types;
pub mod sas;
pub mod schema;
pub mod submission;
pub mod temporal_arithmetic;
pub mod temporal_imputation;
pub mod text_case;
pub mod text_extract;
pub mod text_pad;
pub mod text_template;
pub mod verification;
pub mod yaml_source;

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

/// A text context value.
pub fn text(value: &str) -> Value {
    Value::String(value.into())
}

/// Assert that `finding` is the record its author wrote: phase, condition,
/// rule, spec paths and context, with no source span and no operand route.
/// The fields are compared as one value, so a failure shows all of them, at
/// the caller's line. Returns the rendered cause.
#[track_caller]
pub fn expect(
    finding: &Diagnostic,
    phase: &str,
    condition: &str,
    rule: Option<&str>,
    paths: &[&str],
    context: Vec<(&str, Value)>,
) -> ConditionCode {
    let definition = finding.definition();
    let spec_paths: Vec<&str> = finding.spec_paths.iter().map(String::as_str).collect();
    let context: yamaa_core::diagnostic::Context = context
        .into_iter()
        .map(|(name, value)| (name.into(), value))
        .collect();
    assert_eq!(
        (
            definition.phase,
            definition.condition,
            definition.rule,
            spec_paths,
            &finding.context,
            &finding.source_span,
            &finding.operand_route,
        ),
        (
            phase,
            condition,
            rule,
            paths.to_vec(),
            &context,
            &None,
            &None
        ),
    );
    rendered(finding)
}

/// Every cause this thread rendered from a reached finding.
pub fn rendered_causes() -> BTreeSet<ConditionCode> {
    RENDERED.with(|causes| causes.borrow().clone())
}
impl<T: Record> Record for &T {
    fn record(&self) -> Diagnostic {
        (**self).record()
    }
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
