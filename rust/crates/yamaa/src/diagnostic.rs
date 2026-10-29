//! Facade-owned diagnostics: typed records, never JSON text or tuples.
//!
//! This is the edge where core's diagnostic record (cause, spec paths,
//! structured context and optional geometry) becomes its public form: the
//! registry names the phase, condition, rule and severity, core renders the
//! message from the registry's template, and JSON text is made here and
//! nowhere earlier: the issues table's canonical `context` column, and the
//! issue objects of [`Diagnostic::to_value`], whose `context` is an object.
//! The one record feeds every form of an issue: the table row, the printed
//! line ([`Diagnostic::text`]), the error text and the JSON object. The
//! registry gives every issue error severity: only error-severity findings
//! become issues, while warning-severity findings surface as rows of the run
//! report. Context numbers keep their exact identity: an integer token that
//! `i64` does not hold keeps its decimal spelling instead of passing through
//! binary64. The canonical context text is retained privately so host
//! data-frame conversion stays byte-identical; the public shape is the value
//! map. Geometry and location are optional, and absent means unavailable: a
//! present zero-width span or a present empty route is a recorded fact, never
//! a default.

use std::collections::BTreeMap;

pub(crate) use yamaa_application::issue_location::Locator;

use crate::conditions::ConditionCode;
use crate::table::Table;

// The host tables' columns and the closed vocabularies of their values.
// rust/interface/bindings.yaml declares each of these constants, and its
// generator refuses a column or value that the declaration lacks.

/// The columns of the issues table that [`issues`] builds, in order
/// (D28): the frozen twelve of #3030 section 2.7.
pub const ISSUE_COLUMNS: [&str; 12] = [
    "severity",
    "phase",
    "condition",
    "rule",
    "spec_paths",
    "file",
    "line",
    "column",
    "message",
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

/// The phase that reported an issue: one of the twelve phases the
/// registered conditions name, in run order, as [`PHASES`] writes them.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    Validation,
    Planning,
    Ingest,
    RowConstruction,
    Join,
    Mapping,
    Impute,
    Derivation,
    Convert,
    Verification,
    Output,
    Engine,
}

/// The phases in run order, one for each entry of [`PHASES`].
const PHASE_ORDER: [Phase; 12] = [
    Phase::Validation,
    Phase::Planning,
    Phase::Ingest,
    Phase::RowConstruction,
    Phase::Join,
    Phase::Mapping,
    Phase::Impute,
    Phase::Derivation,
    Phase::Convert,
    Phase::Verification,
    Phase::Output,
    Phase::Engine,
];

impl Phase {
    /// The phase as written, such as `row_construction`: one of [`PHASES`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Planning => "planning",
            Self::Ingest => "ingest",
            Self::RowConstruction => "row_construction",
            Self::Join => "join",
            Self::Mapping => "mapping",
            Self::Impute => "impute",
            Self::Derivation => "derivation",
            Self::Convert => "convert",
            Self::Verification => "verification",
            Self::Output => "output",
            Self::Engine => "engine",
        }
    }

    /// The phase written `text`, or none for text that names none of the
    /// twelve phases.
    pub fn parse(text: &str) -> Option<Self> {
        PHASE_ORDER.into_iter().find(|phase| phase.as_str() == text)
    }
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The columns of the run report, in order.
pub const REPORT_COLUMNS: &[&str] = yamaa_core::run_report::COLUMNS;

/// Values of the run report's `KIND` column, in report order.
pub const REPORT_KINDS: [&str; 6] = yamaa_core::run_report::KINDS;

/// Values of the run report's `OUTCOME` column.
pub const REPORT_OUTCOMES: [&str; 4] = yamaa_core::run_report::OUTCOMES;

/// Values of the issues table's `severity` column, one for each
/// [`Severity`], and of the run report's `SEVERITY` column, the severity of a
/// check.
pub const SEVERITIES: [&str; 2] = yamaa_core::dataset::SEVERITIES;

/// Diagnostic severity. Every issue carries error severity today.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    /// The severity as the issues table's `severity` column writes it:
    /// `error` or `warning`, one of [`SEVERITIES`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

/// Where an issue's first spec path is written (P-ISSUE-1): the YAML file of
/// the layer whose value took effect, and the node's position in it. The
/// issues table's `file`, `line` and `column` columns show it.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    /// The file's absolute path, with forward slashes.
    pub file: String,
    /// The node's position in the file, when a node was located.
    pub position: Option<Position>,
}

/// A one-based line and column in a file; the column counts Unicode scalar
/// values.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
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
    /// A number token that `i64` and `f64` do not hold exactly, kept as its
    /// exact spelling: an integer token outside the signed 64-bit range, which
    /// never passes through `f64`, or a fraction or exponent token outside
    /// `f64`'s finite range.
    Wide(String),
    /// A fraction or exponent token, as binary64.
    Float(f64),
    Text(String),
    List(Vec<ContextValue>),
    Map(BTreeMap<String, ContextValue>),
}

impl ContextValue {
    /// The value as canonical JSON text (RFC 8785, one line): members in key
    /// order, numbers as their exact tokens, the encoding of the issues
    /// table's `context` column. A float that is not finite writes `null`,
    /// and a [`ContextValue::Wide`] spelling that is not a JSON number
    /// writes as text.
    pub fn to_json(&self) -> String {
        yamaa_adapters::json_wire::encode(&context_value_to_core(self))
    }
}

pub(crate) fn context_value_to_core(value: &ContextValue) -> yamaa_core::json::Value {
    use yamaa_core::json::{Number, Value};
    match value {
        ContextValue::Null => Value::Null,
        ContextValue::Bool(value) => Value::Bool(*value),
        ContextValue::Int(value) => Value::from(*value),
        ContextValue::Wide(text) => text
            .parse::<Number>()
            .map_or_else(|_| Value::String(text.clone()), Value::Number),
        ContextValue::Float(value) => Number::from_f64(*value).map_or(Value::Null, Value::Number),
        ContextValue::Text(text) => Value::String(text.clone()),
        ContextValue::List(values) => {
            Value::Array(values.iter().map(context_value_to_core).collect())
        }
        ContextValue::Map(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), context_value_to_core(value)))
                .collect(),
        ),
    }
}

pub(crate) fn context_value_from_core(value: &yamaa_core::json::Value) -> ContextValue {
    use yamaa_core::json::Value;
    match value {
        Value::Null => ContextValue::Null,
        Value::Bool(value) => ContextValue::Bool(*value),
        Value::Number(value) => {
            // Integer tokens never pass through binary64: an integer that
            // `i64` does not hold keeps its spelling.
            if value.is_integer() {
                value.as_i64().map_or_else(
                    || ContextValue::Wide(value.as_str().to_owned()),
                    ContextValue::Int,
                )
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
/// issues table, one printed line and one JSON object.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// The phase that reported the issue.
    pub phase: Phase,
    /// The condition, as the issues table's `condition` column names it.
    pub code: ConditionCode,
    pub rule: Option<String>,
    pub spec_path: Vec<String>,
    /// Where the first spec path is written (P-ISSUE-1): the YAML file of
    /// the layer whose value took effect, at the key of the deepest written
    /// mapping member or the first character of the deepest written list
    /// item, or, for `invalid_yaml` and `non_ascii_source`, the file the YAML
    /// reader refused at the position it reported. `None`, and the table's
    /// `file`, `line` and `column` missing, when the issue has no spec path
    /// or no file the operation read holds it, such as a path that names a
    /// data file's record or a written artifact.
    pub location: Option<Location>,
    /// The issue's message, rendered in Rust from the template the condition
    /// registry holds for its condition. Messages are not frozen: match on
    /// the condition and rule, never on the message.
    pub message: String,
    pub context: DiagnosticContext,
    /// Byte span in the original expression, when the failure site recorded one.
    pub source_span: Option<SourceSpan>,
    /// Operand steps from the expression root to the failing node, in recorded
    /// order, when the failure site recorded them. A present empty route is a
    /// failure at the root, not an absent route.
    pub operand_route: Option<Vec<String>>,
}

/// The severity of one issue, as the registry gives its condition.
pub(crate) fn severity(diagnostic: &yamaa_core::diagnostic::Diagnostic) -> Severity {
    from_core(diagnostic.severity())
}

/// The public form of a severity the registry gives.
pub(crate) fn from_core(severity: yamaa_core::diagnostic::Severity) -> Severity {
    match severity {
        yamaa_core::diagnostic::Severity::Error => Severity::Error,
    }
}

impl Diagnostic {
    /// The public form of `diagnostic`, located by `locator`: the documents
    /// of the operation that found it.
    pub(crate) fn located(
        diagnostic: &yamaa_core::diagnostic::Diagnostic,
        locator: &Locator,
    ) -> Self {
        let definition = diagnostic.definition();
        Self {
            severity: severity(diagnostic),
            phase: Phase::parse(definition.phase)
                .expect("a registered condition names one of the twelve phases"),
            code: ConditionCode::parse(definition.condition)
                .expect("every condition core reports is registered"),
            rule: definition.rule.map(str::to_owned),
            spec_path: diagnostic.spec_paths.clone(),
            location: locator.locate(diagnostic).map(|location| Location {
                file: location.file,
                position: Some(Position {
                    line: location.line,
                    column: location.column,
                }),
            }),
            message: diagnostic.message(),
            context: DiagnosticContext::from_core(&diagnostic.context),
            source_span: diagnostic.source_span.map(|span| SourceSpan {
                start: span.start,
                end: span.end,
            }),
            operand_route: diagnostic.operand_route.clone(),
        }
    }

    /// The issue as one printed line (P-PRINT-1 rule 3): `<location>:
    /// <message> [<condition>, <rule>]`, the location's file relative to the
    /// current directory when beneath it and its missing parts dropped,
    /// `[<condition>]` when the condition cites no rule, and only ASCII. The
    /// command line prints each issue so; a summary's `first issue:` line and
    /// D39's error text carry the first.
    pub fn text(&self) -> String {
        crate::text::issue_line(self, &crate::text::current_directory())
    }

    /// The issue as one JSON object with the twelve keys of the issues table,
    /// whose `context` is an object rather than text, as the command line's
    /// `--format json` writes it; [`ContextValue::to_json`] encodes it.
    /// Missing cells are `null`.
    pub fn to_value(&self) -> ContextValue {
        let text = |value: &str| ContextValue::Text(value.to_owned());
        let texts =
            |values: &[String]| ContextValue::List(values.iter().map(|v| text(v)).collect());
        let int = |value: usize| ContextValue::Int(i64::try_from(value).unwrap_or(i64::MAX));
        let position = self
            .location
            .as_ref()
            .and_then(|location| location.position);
        let values = [
            ("severity", text(self.severity.as_str())),
            ("phase", text(self.phase.as_str())),
            ("condition", text(self.code.as_str())),
            (
                "rule",
                self.rule.as_deref().map_or(ContextValue::Null, text),
            ),
            ("spec_paths", texts(&self.spec_path)),
            (
                "file",
                self.location
                    .as_ref()
                    .map_or(ContextValue::Null, |location| text(&location.file)),
            ),
            (
                "line",
                position.map_or(ContextValue::Null, |at| int(at.line)),
            ),
            (
                "column",
                position.map_or(ContextValue::Null, |at| int(at.column)),
            ),
            ("message", text(&self.message)),
            ("context", ContextValue::Map(self.context.values.clone())),
            (
                "source_span",
                self.source_span.map_or(ContextValue::Null, |span| {
                    ContextValue::List(vec![int(span.start), int(span.end)])
                }),
            ),
            (
                "operand_route",
                self.operand_route
                    .as_deref()
                    .map_or(ContextValue::Null, texts),
            ),
        ];
        ContextValue::Map(
            values
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }
}

/// The issues of an operation as typed records, first issue first: what a
/// result's `diagnostics()` and `issues()` read, and what a refusal carries.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Report {
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl Report {
    /// The report of issues that no YAML file the operation read holds, such
    /// as a refusal before any file was read: none is located.
    pub(crate) fn from_issues(issues: &[yamaa_core::diagnostic::Diagnostic]) -> Self {
        Self::located(issues, &Locator::none())
    }

    /// The report of `issues`, each located by `locator` (P-ISSUE-1).
    pub(crate) fn located(
        issues: &[yamaa_core::diagnostic::Diagnostic],
        locator: &Locator,
    ) -> Self {
        Self {
            diagnostics: issues
                .iter()
                .map(|issue| Diagnostic::located(issue, locator))
                .collect(),
        }
    }

    /// Return whether the report holds no diagnostics.
    pub(crate) fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Return the number of diagnostics in the report.
    pub(crate) fn len(&self) -> usize {
        self.diagnostics.len()
    }

    /// The issues table of the report's diagnostics, as [`issues`] builds
    /// it.
    pub(crate) fn table(&self) -> Table {
        issues(&self.diagnostics)
    }
}

/// The issues table both hosts return (D28) for `diagnostics`: one row per
/// issue, in order, with the [`ISSUE_COLUMNS`], as a result's `issues()`
/// gives it for its `diagnostics()`. `message` is the rendered message and
/// `context` the canonical JSON text; `rule` is missing for a condition that
/// cites none; `file`, `line` and `column` are missing where no location was
/// recorded; `source_span` is `[start, end]` and, like `operand_route`,
/// missing when the failure site recorded no geometry. Given no issue, it is
/// the table of no issues, which a host attaches to a refusal that no issue
/// explains.
pub fn issues(diagnostics: &[Diagnostic]) -> Table {
    use yamaa_adapters::public_table::{List, PublicColumn, PublicTable};

    let int = |value: usize| i64::try_from(value).unwrap_or(i64::MAX);
    let text = |cell: fn(&Diagnostic) -> Option<&str>| {
        PublicColumn::Str(diagnostics.iter().map(cell).collect())
    };
    let position = |diagnostic: &Diagnostic| {
        diagnostic
            .location
            .as_ref()
            .and_then(|location| location.position)
    };
    let lists = |list: List| PublicColumn::List(Box::new(list));
    let columns: [PublicColumn; ISSUE_COLUMNS.len()] = [
        text(|diagnostic| Some(diagnostic.severity.as_str())),
        text(|diagnostic| Some(diagnostic.phase.as_str())),
        text(|diagnostic| Some(diagnostic.code.as_str())),
        text(|diagnostic| diagnostic.rule.as_deref()),
        lists(List::text(diagnostics.iter().map(|diagnostic| {
            Some(diagnostic.spec_path.iter().map(String::as_str).collect())
        }))),
        text(|diagnostic| {
            diagnostic
                .location
                .as_ref()
                .map(|location| location.file.as_str())
        }),
        PublicColumn::Int(
            diagnostics
                .iter()
                .map(|diagnostic| position(diagnostic).map(|at| int(at.line)))
                .collect(),
        ),
        PublicColumn::Int(
            diagnostics
                .iter()
                .map(|diagnostic| position(diagnostic).map(|at| int(at.column)))
                .collect(),
        ),
        text(|diagnostic| Some(&diagnostic.message)),
        text(|diagnostic| Some(diagnostic.context.to_json())),
        lists(List::int(diagnostics.iter().map(|diagnostic| {
            diagnostic
                .source_span
                .map(|span| vec![int(span.start), int(span.end)])
        }))),
        lists(List::text(diagnostics.iter().map(|diagnostic| {
            diagnostic
                .operand_route
                .as_ref()
                .map(|route| route.iter().map(String::as_str).collect())
        }))),
    ];
    let columns = ISSUE_COLUMNS
        .into_iter()
        .map(str::to_owned)
        .zip(columns)
        .collect();
    Table::from_public(PublicTable::new(columns).expect("the issue columns form one table"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: tools/validation/check_diagnostics.py forbids registered
    // condition/rule literals in crate sources (they live in the core
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
        assert_eq!(diagnostic.phase.as_str(), CODE.definition().phase);
        assert_eq!(diagnostic.code.as_str(), CODE.definition().condition);
        assert_eq!(diagnostic.rule.as_deref(), None);
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
            Some(&ContextValue::Wide("18446744073709551615".into()))
        );
        assert_eq!(
            diagnostic.context.get("ratio"),
            Some(&ContextValue::Float(1.5))
        );
        // Integers outside the signed 64-bit range keep their exact
        // spelling and stay numbers, never text and never a rounded binary64.
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

    /// The record's message renders its condition's registered template
    /// with the record's values, and its severity is the registry's.
    #[test]
    fn messages_and_severity_come_from_the_registry() {
        let text = |value: &str| yamaa_core::json::Value::String(value.into());
        let mut forward = issue();
        forward.code = yamaa_core::diagnostic::ConditionCode::ColumnForwardReference;
        forward.context = [
            ("column".into(), text("ASTDY")),
            ("dependency".into(), text("TRTSDT")),
        ]
        .into();
        let report = Report::from_issues(&[forward]);
        assert_eq!(
            report.diagnostics[0].message,
            "column ASTDY reads TRTSDT, which is declared after it"
        );
        assert_eq!(report.diagnostics[0].severity, Severity::Error);
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
    #[cfg(feature = "arrow")]
    fn issues_table_keeps_every_field_and_absent_geometry() {
        use arrow_array::{Array, Int64Array, ListArray, StringArray};
        let mut located = issue();
        located.code = yamaa_core::diagnostic::ConditionCode::IntegerOverflow;
        located.source_span = Some(yamaa_core::expression::SourceSpan { start: 3, end: 9 });
        located.operand_route = Some(vec!["left".into()]);
        let mut report = Report::from_issues(&[issue(), located]);
        report.diagnostics[1].location = Some(Location {
            file: "/study/sdtm/dm.yaml".into(),
            position: Some(Position {
                line: 41,
                column: 5,
            }),
        });
        let table = report.table();
        let names: Vec<_> = table.columns().iter().map(|f| f.name().as_str()).collect();
        assert_eq!(
            names,
            [
                "severity",
                "phase",
                "condition",
                "rule",
                "spec_paths",
                "file",
                "line",
                "column",
                "message",
                "context",
                "source_span",
                "operand_route"
            ]
        );
        assert_eq!(table.rows(), 2);
        let batch = &table.batches()[0];
        let column = |name: &str| batch.column(table.schema().index_of(name).unwrap()).clone();
        let text = |name: &str| {
            column(name)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .clone()
        };
        let int = |name: &str| {
            column(name)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .clone()
        };
        let list = |name: &str| {
            column(name)
                .as_any()
                .downcast_ref::<ListArray>()
                .unwrap()
                .clone()
        };
        let overflow = yamaa_core::diagnostic::ConditionCode::IntegerOverflow.definition();
        assert_eq!(text("severity").value(0), "error");
        assert_eq!(text("phase").value(0), CODE.definition().phase);
        assert_eq!(text("condition").value(1), overflow.condition);
        assert!(text("rule").is_null(0));
        assert_eq!(Some(text("rule").value(1)), overflow.rule);
        let paths = list("spec_paths").value(0);
        let paths = paths.as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(paths.value(0), "columns.A");
        // An issue no file the operation read locates has the three columns
        // missing; a located one has all three.
        assert!(text("file").is_null(0) && int("line").is_null(0) && int("column").is_null(0));
        assert_eq!(text("file").value(1), "/study/sdtm/dm.yaml");
        assert_eq!((int("line").value(1), int("column").value(1)), (41, 5));
        assert_eq!(text("message").value(0), report.diagnostics[0].message);
        assert!(!text("message").value(1).is_empty());
        assert_eq!(text("context").value(0), CONTEXT);
        assert!(list("source_span").is_null(0));
        let span = list("source_span").value(1);
        let span = span.as_any().downcast_ref::<Int64Array>().unwrap();
        assert_eq!((span.value(0), span.value(1)), (3, 9));
        assert!(list("operand_route").is_null(0));
        assert!(!list("operand_route").is_null(1));
    }

    /// An issue's JSON object has the table's twelve keys, with `context` an
    /// object whose canonical text is the table's `context` cell.
    #[test]
    fn an_issue_as_json_carries_its_context_as_an_object() {
        let mut located = issue();
        located.source_span = Some(yamaa_core::expression::SourceSpan { start: 0, end: 4 });
        located.operand_route = Some(Vec::new());
        let report = Report::from_issues(&[located]);
        let diagnostic = &report.diagnostics[0];
        let ContextValue::Map(value) = diagnostic.to_value() else {
            panic!("an issue is an object");
        };
        let keys = value.keys().map(String::as_str).collect::<Vec<_>>();
        let mut columns = ISSUE_COLUMNS.to_vec();
        columns.sort_unstable();
        assert_eq!(keys, columns);
        assert_eq!(value["context"].to_json(), CONTEXT);
        assert_eq!(value["context"].to_json(), diagnostic.context.to_json());
        let message = diagnostic.message.clone();
        let condition = CODE.definition().condition;
        let phase = CODE.definition().phase;
        assert_eq!(
            diagnostic.to_value().to_json(),
            format!(
                r#"{{"column":null,"condition":"{condition}","context":{CONTEXT},"file":null,"line":null,"message":"{message}","operand_route":[],"phase":"{phase}","rule":null,"severity":"error","source_span":[0,4],"spec_paths":["columns.A"]}}"#
            )
        );
    }

    #[test]
    fn public_values_encode_as_canonical_json() {
        let value = ContextValue::Map(
            [
                ("b".to_owned(), ContextValue::Float(1.5)),
                ("a".to_owned(), ContextValue::Wide("1e400".to_owned())),
                ("c".to_owned(), ContextValue::Float(f64::NAN)),
                (
                    "d".to_owned(),
                    ContextValue::Wide("not a number".to_owned()),
                ),
                (
                    "e".to_owned(),
                    ContextValue::List(vec![
                        ContextValue::Wide("18446744073709551615".to_owned()),
                        ContextValue::Int(-1),
                        ContextValue::Text("\"é\"\n".to_owned()),
                        ContextValue::Bool(false),
                    ]),
                ),
            ]
            .into(),
        );
        assert_eq!(
            value.to_json(),
            r#"{"a":1e400,"b":1.5,"c":null,"d":"not a number","e":[18446744073709551615,-1,"\"é\"\n",false]}"#
        );
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
    fn phases_are_the_phases_in_order() {
        for phase in PHASE_ORDER {
            // Exhaustive: a new phase fails to compile here until it is
            // listed in PHASE_ORDER, and so in PHASES.
            match phase {
                Phase::Validation
                | Phase::Planning
                | Phase::Ingest
                | Phase::RowConstruction
                | Phase::Join
                | Phase::Mapping
                | Phase::Impute
                | Phase::Derivation
                | Phase::Convert
                | Phase::Verification
                | Phase::Output
                | Phase::Engine => {}
            }
            assert_eq!(Phase::parse(phase.as_str()), Some(phase));
        }
        assert_eq!(PHASE_ORDER.map(Phase::as_str), PHASES);
        assert!(PHASE_ORDER.windows(2).all(|pair| pair[0] < pair[1]));
    }

    /// Every cause core can report converts: its phase is one of the twelve
    /// and its condition is registered, so a diagnostic's typed fields name
    /// what core's record names.
    #[test]
    fn every_cause_core_reports_has_a_typed_phase_and_condition() {
        for cause in yamaa_core::diagnostic::CONDITIONS {
            let definition = cause.definition();
            let phase = Phase::parse(definition.phase);
            let code = ConditionCode::parse(definition.condition);
            assert_eq!(
                (phase.map(Phase::as_str), code.map(ConditionCode::as_str)),
                (Some(definition.phase), Some(definition.condition)),
                "{cause:?}"
            );
        }
    }

    #[test]
    fn severities_are_the_issue_severities_in_order() {
        assert_eq!(
            [Severity::Error, Severity::Warning].map(Severity::as_str),
            SEVERITIES
        );
    }

    #[test]
    fn empty_reports_report_emptiness() {
        let report = Report::from_issues(&[]);
        assert!(report.is_empty());
        assert_eq!(report.len(), 0);
    }
}
