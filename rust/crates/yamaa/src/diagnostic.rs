//! Facade-owned diagnostics: typed records, never JSON text or tuples.
//!
//! This is the edge where core's diagnostic record (cause, spec paths,
//! structured context and optional geometry) becomes its public form: the
//! registry names the phase, condition and requirement, and the context
//! becomes JSON text here and nowhere earlier. Severity is error for every
//! issue today: only error-severity findings become issues, while
//! warning-severity findings surface as rows of the run report. Context
//! numbers keep their exact identity: an integer token wider than both 64-bit
//! machine types keeps its decimal spelling instead of passing through
//! binary64. The canonical context text is retained privately so host
//! data-frame conversion stays byte-identical; the public shape is the value
//! map. Geometry is optional, and absent means unavailable: a present
//! zero-width span or a present empty route is a recorded fact, never a
//! default.

use std::collections::BTreeMap;

use crate::table::Table;

// The host tables' columns and the closed vocabularies of their values.
// rust/interface/bindings.yaml declares each of these constants, and its
// generator refuses a column or value that the declaration lacks.

/// The columns of the issues table that [`Report::table`] builds, in order.
pub const ISSUE_COLUMNS: [&str; 7] = [
    "phase",
    "condition",
    "requirement",
    "spec_paths",
    "context",
    "source_span",
    "operand_route",
];

/// Every phase an issue names, in run order: the phases of the registered
/// conditions.
pub const PHASES: [&str; 12] = [
    "validation",
    "planning",
    "ingest",
    "row_construction",
    "join",
    "mapping",
    "impute",
    "derivation",
    "convert",
    "verification",
    "output",
    "engine",
];

/// The columns of the run report, in order.
pub const REPORT_COLUMNS: &[&str] = yamaa_core::run_report::COLUMNS;

/// Values of the run report's `KIND` column, in report order.
pub const REPORT_KINDS: [&str; 6] = yamaa_core::run_report::KINDS;

/// Values of the run report's `OUTCOME` column.
pub const REPORT_OUTCOMES: [&str; 4] = yamaa_core::run_report::OUTCOMES;

/// Values of the run report's `SEVERITY` column: the severity of a check.
pub const SEVERITIES: [&str; 2] = yamaa_core::dataset::SEVERITIES;

/// Diagnostic severity. Every issue carries error severity today.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// Half-open UTF-8 byte offsets into the original expression text.
///
/// Present only when the failure site recorded geometry; `start == end == 0`
/// is a real zero-width span at the first byte, not an absent span.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

/// A structured diagnostic context value, the public form of the JSON value.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum ContextValue {
    Null,
    Bool(bool),
    /// An integer token within the signed 64-bit range.
    Int(i64),
    /// An integer token above `i64::MAX` within the unsigned 64-bit range.
    Uint(u64),
    /// A number token no machine type here holds exactly, kept as its exact
    /// spelling: an integer outside both 64-bit ranges, or a fraction or
    /// exponent token outside binary64's finite range.
    Wide(String),
    /// A fraction or exponent token, as binary64.
    Float(f64),
    Text(String),
    List(Vec<ContextValue>),
    Map(BTreeMap<String, ContextValue>),
}

pub(crate) fn context_value_from_core(value: &yamaa_core::json::Value) -> ContextValue {
    use yamaa_core::json::Value;
    match value {
        Value::Null => ContextValue::Null,
        Value::Bool(value) => ContextValue::Bool(*value),
        Value::Number(value) => {
            // Integer tokens never pass through binary64: an integer that
            // fits neither 64-bit type keeps its spelling.
            if value.is_integer() {
                if let Some(number) = value.as_i64() {
                    ContextValue::Int(number)
                } else if let Some(number) = value.as_u64() {
                    ContextValue::Uint(number)
                } else {
                    ContextValue::Wide(value.as_str().to_owned())
                }
            } else if let Some(number) = value.as_f64() {
                ContextValue::Float(number)
            } else {
                ContextValue::Wide(value.as_str().to_owned())
            }
        }
        Value::String(value) => ContextValue::Text(value.clone()),
        Value::Array(values) => {
            ContextValue::List(values.iter().map(context_value_from_core).collect())
        }
        Value::Object(values) => ContextValue::Map(
            values
                .iter()
                .map(|(key, value)| (key.clone(), context_value_from_core(value)))
                .collect(),
        ),
    }
}

/// A structured diagnostic context map, the public form of the issue context.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosticContext {
    values: BTreeMap<String, ContextValue>,
    text: String,
}

impl DiagnosticContext {
    pub(crate) fn from_core(context: &yamaa_core::diagnostic::Context) -> Self {
        Self {
            values: context
                .iter()
                .map(|(key, value)| (key.clone(), context_value_from_core(value)))
                .collect(),
            text: yamaa_adapters::json_wire::encode_object(context),
        }
    }

    /// Borrow one structured context value by key.
    pub fn get(&self, key: &str) -> Option<&ContextValue> {
        self.values.get(key)
    }

    /// Iterate over the structured context entries in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &ContextValue)> {
        self.values.iter()
    }

    /// Return the number of structured context entries.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Return whether the structured context holds no entries.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Return the context JSON text, the canonical encoding of the issue
    /// context tree. Host data-frame conversion uses this text; embedders
    /// use the value map.
    pub fn to_json(&self) -> &str {
        &self.text
    }
}

/// A typed diagnostic record, the public form of one issue: one row of the
/// issues table.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub phase: String,
    pub code: String,
    pub requirement: Option<String>,
    pub spec_path: Vec<String>,
    pub context: DiagnosticContext,
    /// Byte span in the original expression, when the failure site recorded one.
    pub source_span: Option<SourceSpan>,
    /// Operand steps from the expression root to the failing node, in recorded
    /// order, when the failure site recorded them. A present empty route is a
    /// failure at the root, not an absent route.
    pub operand_route: Option<Vec<String>>,
}

impl Diagnostic {
    pub(crate) fn from_core(diagnostic: &yamaa_core::diagnostic::Diagnostic) -> Self {
        let definition = diagnostic.definition();
        Self {
            severity: Severity::Error,
            phase: definition.phase.to_owned(),
            code: definition.condition.to_owned(),
            requirement: definition.requirement.map(str::to_owned),
            spec_path: diagnostic.spec_paths.clone(),
            context: DiagnosticContext::from_core(&diagnostic.context),
            source_span: diagnostic.source_span.map(|span| SourceSpan {
                start: span.start,
                end: span.end,
            }),
            operand_route: diagnostic.operand_route.clone(),
        }
    }
}

/// An owned diagnostic report, the public form of the issue-row collection.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub(crate) fn from_issues(issues: &[yamaa_core::diagnostic::Diagnostic]) -> Self {
        Self {
            diagnostics: issues.iter().map(Diagnostic::from_core).collect(),
        }
    }

    /// Return whether the report holds no diagnostics.
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Return the number of diagnostics in the report.
    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }

    /// The issues table both hosts return: one row per diagnostic, in report
    /// order. `context` is the canonical JSON text; `source_span` is
    /// `[start, end]` and, like `operand_route`, missing when the failure site
    /// recorded no geometry.
    pub fn table(&self) -> Table {
        use arrow_array::builder::{Int64Builder, ListBuilder, StringBuilder};
        use arrow_schema::{DataType, Field, Schema};
        use std::sync::Arc;

        let mut phase = StringBuilder::new();
        let mut condition = StringBuilder::new();
        let mut requirement = StringBuilder::new();
        let mut spec_paths = ListBuilder::new(StringBuilder::new());
        let mut context = StringBuilder::new();
        let mut source_span = ListBuilder::new(Int64Builder::new());
        let mut operand_route = ListBuilder::new(StringBuilder::new());
        for diagnostic in &self.diagnostics {
            phase.append_value(&diagnostic.phase);
            condition.append_value(&diagnostic.code);
            requirement.append_option(diagnostic.requirement.as_deref());
            for path in &diagnostic.spec_path {
                spec_paths.values().append_value(path);
            }
            spec_paths.append(true);
            context.append_value(diagnostic.context.to_json());
            match diagnostic.source_span {
                Some(span) => {
                    for offset in [span.start, span.end] {
                        source_span
                            .values()
                            .append_value(i64::try_from(offset).unwrap_or(i64::MAX));
                    }
                    source_span.append(true);
                }
                None => source_span.append(false),
            }
            match &diagnostic.operand_route {
                Some(route) => {
                    for step in route {
                        operand_route.values().append_value(step);
                    }
                    operand_route.append(true);
                }
                None => operand_route.append(false),
            }
        }
        let list = |item: DataType| DataType::List(Arc::new(Field::new("item", item, true)));
        let types: [DataType; ISSUE_COLUMNS.len()] = [
            DataType::Utf8,
            DataType::Utf8,
            DataType::Utf8,
            list(DataType::Utf8),
            DataType::Utf8,
            list(DataType::Int64),
            list(DataType::Utf8),
        ];
        let schema = Arc::new(Schema::new(
            ISSUE_COLUMNS
                .into_iter()
                .zip(types)
                .map(|(name, kind)| Field::new(name, kind, true))
                .collect::<Vec<_>>(),
        ));
        let batch = arrow_array::RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(phase.finish()),
                Arc::new(condition.finish()),
                Arc::new(requirement.finish()),
                Arc::new(spec_paths.finish()),
                Arc::new(context.finish()),
                Arc::new(source_span.finish()),
                Arc::new(operand_route.finish()),
            ],
        )
        .expect("the builders produce exactly the declared issue columns");
        Table::from_public(
            yamaa_adapters::public_table::PublicTable::from_batches(schema, vec![batch])
                .expect("the batch was built against this schema"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: tools/validation/check_diagnostics.py forbids registered
    // condition/requirement literals in crate sources (they live in the core
    // registry and in tests/ instead), so the expected vocabulary below is read
    // from the registry. tests/diagnostic_records.rs covers real registry
    // diagnostics end to end through the public Engine API.
    const CONTEXT: &str = r#"{"beyond":18446744073709551615,"maximum":9223372036854775807,"minimum":-9223372036854775808,"ratio":1.5,"values":[null,"é🙂",true],"wide":[-18446744073709551617,340282366920938463463374607431768211456]}"#;
    const CODE: yamaa_core::diagnostic::ConditionCode =
        yamaa_core::diagnostic::ConditionCode::ApplicationRejected;

    fn issue() -> yamaa_core::diagnostic::Diagnostic {
        let yamaa_core::json::Value::Object(context) =
            yamaa_adapters::json_wire::decode(CONTEXT).unwrap()
        else {
            panic!("object context fixture")
        };
        yamaa_core::diagnostic::Diagnostic {
            code: CODE,
            spec_paths: vec!["columns.A".into()],
            context,
            source_span: None,
            operand_route: None,
        }
    }

    #[test]
    fn diagnostics_keep_typed_records_and_original_text() {
        let report = Report::from_issues(&[issue()]);
        assert_eq!(report.len(), 1);
        assert!(!report.is_empty());
        let diagnostic = &report.diagnostics[0];
        assert_eq!(diagnostic.severity, Severity::Error);
        assert_eq!(diagnostic.phase, CODE.definition().phase);
        assert_eq!(diagnostic.code, CODE.definition().condition);
        assert_eq!(diagnostic.requirement.as_deref(), None);
        assert_eq!(diagnostic.spec_path, ["columns.A"]);
        assert_eq!(diagnostic.context.to_json(), CONTEXT);
        assert_eq!(
            diagnostic.context.get("minimum"),
            Some(&ContextValue::Int(i64::MIN))
        );
        assert_eq!(
            diagnostic.context.get("maximum"),
            Some(&ContextValue::Int(i64::MAX))
        );
        assert_eq!(
            diagnostic.context.get("beyond"),
            Some(&ContextValue::Uint(u64::MAX))
        );
        assert_eq!(
            diagnostic.context.get("ratio"),
            Some(&ContextValue::Float(1.5))
        );
        // Integers outside both 64-bit ranges keep their exact spelling and
        // stay numbers, never text and never a rounded binary64.
        assert_eq!(
            diagnostic.context.get("wide"),
            Some(&ContextValue::List(vec![
                ContextValue::Wide("-18446744073709551617".into()),
                ContextValue::Wide("340282366920938463463374607431768211456".into()),
            ]))
        );
        assert_eq!(
            diagnostic.context.get("values"),
            Some(&ContextValue::List(vec![
                ContextValue::Null,
                ContextValue::Text("é🙂".into()),
                ContextValue::Bool(true),
            ]))
        );
        assert_eq!(diagnostic.context.len(), 6);
        assert!(!diagnostic.context.is_empty());
        assert!(diagnostic.context.get("absent").is_none());
        assert_eq!(diagnostic.source_span, None);
        assert_eq!(diagnostic.operand_route, None);
    }

    #[test]
    fn present_zero_geometry_differs_from_absent_geometry() {
        let mut zero = issue();
        zero.source_span = Some(yamaa_core::expression::SourceSpan { start: 0, end: 0 });
        zero.operand_route = Some(Vec::new());
        let mut located = issue();
        located.source_span = Some(yamaa_core::expression::SourceSpan { start: 3, end: 9 });
        located.operand_route = Some(vec!["left".into(), "argument:1".into()]);
        let report = Report::from_issues(&[issue(), zero, located]);
        let [absent, zero, located] = report.diagnostics.as_slice() else {
            panic!("three diagnostics");
        };
        assert_eq!(absent.source_span, None);
        assert_eq!(absent.operand_route, None);
        let span = zero.source_span.expect("recorded zero-width span");
        assert_eq!((span.start, span.end), (0, 0));
        assert_eq!(zero.operand_route.as_deref(), Some(&[][..]));
        let span = located.source_span.expect("recorded span");
        assert_eq!((span.start, span.end), (3, 9));
        assert_eq!(
            located.operand_route.as_deref(),
            Some(&["left".to_owned(), "argument:1".to_owned()][..])
        );
        assert_ne!(absent, zero);
        assert_ne!(zero, located);
        // Geometry never changes the retained context text.
        for diagnostic in [absent, zero, located] {
            assert_eq!(diagnostic.context.to_json(), CONTEXT);
        }
    }

    #[test]
    fn an_empty_context_yields_an_empty_map_and_an_empty_object() {
        let mut empty = issue();
        empty.context.clear();
        let report = Report::from_issues(&[empty]);
        let context = &report.diagnostics[0].context;
        assert!(context.is_empty());
        assert_eq!(context.to_json(), "{}");
        assert_eq!(context.iter().count(), 0);
    }

    #[test]
    fn issues_table_keeps_every_field_and_absent_geometry() {
        use arrow_array::{Array, Int64Array, ListArray, StringArray};
        let mut located = issue();
        located.code = yamaa_core::diagnostic::ConditionCode::IntegerOverflow;
        located.source_span = Some(yamaa_core::expression::SourceSpan { start: 3, end: 9 });
        located.operand_route = Some(vec!["left".into()]);
        let table = Report::from_issues(&[issue(), located]).table();
        let names: Vec<_> = table.columns().iter().map(|f| f.name().as_str()).collect();
        assert_eq!(
            names,
            [
                "phase",
                "condition",
                "requirement",
                "spec_paths",
                "context",
                "source_span",
                "operand_route"
            ]
        );
        assert_eq!(table.rows(), 2);
        let batch = &table.batches()[0];
        let text = |index: usize| {
            batch
                .column(index)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .clone()
        };
        let overflow = yamaa_core::diagnostic::ConditionCode::IntegerOverflow.definition();
        assert_eq!(text(0).value(0), CODE.definition().phase);
        assert_eq!(text(1).value(1), overflow.condition);
        assert!(text(2).is_null(0));
        assert_eq!(Some(text(2).value(1)), overflow.requirement);
        assert_eq!(text(4).value(0), CONTEXT);
        let list = |index: usize| {
            batch
                .column(index)
                .as_any()
                .downcast_ref::<ListArray>()
                .unwrap()
                .clone()
        };
        let paths = list(3).value(0);
        let paths = paths.as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(paths.value(0), "columns.A");
        assert!(list(5).is_null(0));
        let span = list(5).value(1);
        let span = span.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!((span.value(0), span.value(1)), (3, 9));
        assert!(list(6).is_null(0));
        assert!(!list(6).is_null(1));
    }

    #[test]
    fn phases_are_the_phases_of_the_registered_conditions() {
        let registered = yamaa_core::diagnostic::CONDITIONS
            .iter()
            .map(|code| code.definition().phase)
            .collect::<std::collections::BTreeSet<_>>();
        let declared = PHASES
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(declared.len(), PHASES.len(), "a phase is listed twice");
        assert_eq!(registered, declared);
    }

    #[test]
    fn empty_reports_report_emptiness() {
        let report = Report::from_issues(&[]);
        assert!(report.is_empty());
        assert_eq!(report.len(), 0);
    }
}
