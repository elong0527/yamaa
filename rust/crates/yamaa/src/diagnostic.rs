//! Facade-owned diagnostics: typed records, never JSON text or tuples.
//!
//! This is the public form of the adapter issue rows (phase, condition,
//! requirement, spec paths, structured context and optional geometry).
//! Severity is error for every record the portable shape carries today: only
//! error-severity findings become issue rows, while warning-severity findings
//! surface as log tables. Context numbers keep their exact identity: an
//! integer token wider than both 64-bit machine types keeps its decimal
//! spelling instead of passing through binary64. The original context text is
//! retained privately so host data-frame conversion stays byte-identical; the
//! public shape is the value map. Geometry is optional, and absent means
//! unavailable: a present zero-width span or a present empty route is a
//! recorded fact, never a default.

use std::collections::BTreeMap;

/// Diagnostic severity. Every portable issue row carries error severity today.
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

fn context_value_from_core(value: &yamaa_core::json::Value) -> ContextValue {
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
    pub(crate) fn from_issue(issue: &yamaa_adapters::issue_rows::Issue) -> Self {
        let values = match &issue.context {
            yamaa_core::json::Value::Object(values) => values
                .iter()
                .map(|(key, value)| (key.clone(), context_value_from_core(value)))
                .collect(),
            _ => BTreeMap::new(),
        };
        Self {
            values,
            text: yamaa_adapters::json_wire::encode(&issue.context),
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

/// A typed diagnostic record, the public form of one issue row.
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
    pub(crate) fn from_issue(issue: &yamaa_adapters::issue_rows::Issue) -> Self {
        Self {
            severity: Severity::Error,
            phase: issue.phase.clone(),
            code: issue.condition.clone(),
            requirement: issue.requirement.clone(),
            spec_path: issue.spec_paths.clone(),
            context: DiagnosticContext::from_issue(issue),
            source_span: issue.source_span.map(|span| SourceSpan {
                start: span.start,
                end: span.end,
            }),
            operand_route: issue.operand_route.clone(),
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
    pub(crate) fn from_issues(issues: &[yamaa_adapters::issue_rows::Issue]) -> Self {
        Self {
            diagnostics: issues.iter().map(Diagnostic::from_issue).collect(),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: the condition below is deliberately NOT a registered condition
    // code and the requirement is None: tools/validation/check_diagnostics.py
    // forbids registered condition/requirement literals in crate sources
    // (they live in the core registry and in tests/ instead). The conversion
    // under test is a pure field mapping, so registry membership is
    // irrelevant here; tests/diagnostic_records.rs covers real registry
    // diagnostics end to end through the public Engine API.
    const CONTEXT: &str = r#"{"beyond":18446744073709551615,"maximum":9223372036854775807,"minimum":-9223372036854775808,"ratio":1.5,"values":[null,"é🙂",true],"wide":[-18446744073709551617,340282366920938463463374607431768211456]}"#;

    fn issue() -> yamaa_adapters::issue_rows::Issue {
        yamaa_adapters::issue_rows::Issue {
            phase: "input".into(),
            condition: "conversion_probe".into(),
            requirement: None,
            spec_paths: vec!["columns.A".into()],
            context: yamaa_adapters::json_wire::decode(CONTEXT).unwrap(),
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
        assert_eq!(diagnostic.phase, "input");
        assert_eq!(diagnostic.code, "conversion_probe");
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
    fn non_object_context_yields_an_empty_map_with_encoded_text() {
        let mut bad = issue();
        bad.context = yamaa_core::json::Value::Null;
        let report = Report::from_issues(&[bad]);
        let context = &report.diagnostics[0].context;
        assert!(context.is_empty());
        assert_eq!(context.to_json(), "null");
        assert_eq!(context.iter().count(), 0);
    }

    #[test]
    fn empty_reports_report_emptiness() {
        let report = Report::from_issues(&[]);
        assert!(report.is_empty());
        assert_eq!(report.len(), 0);
    }
}
