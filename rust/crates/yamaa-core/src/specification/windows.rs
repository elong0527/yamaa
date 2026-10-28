//! Compile original output-window declarations into the existing shared evaluator.
use super::*;
use crate::expression_table::Verb;
use crate::result_type::ResultType;
use crate::{
    bound_expression::BoundPredicate,
    dataset::{OrderTerm, Window, WindowKind},
};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
enum Kind {
    RowNumber,
    Competition,
    Dense,
    RowValue {
        source: String,
        offset: i64,
    },
    Locf {
        source: String,
        include_current: bool,
    },
    BaselineFlag {
        date: String,
        reference_date: String,
        value: String,
        inclusive: bool,
        tie_break: Vec<(String, bool, bool)>,
    },
}
#[derive(Clone, Debug)]
pub(super) struct Declaration {
    kind: Kind,
    operation: String,
    path: String,
    groups: Vec<String>,
    order: Vec<(String, bool, bool)>,
    filter: Option<(String, Result<CompiledExpression, CompileError>)>,
    /// The path of each read, in [`Declaration::operands`] order, as the tree
    /// writes it: `baseline_flag` names its operands `source` and
    /// `reference` and its tie-break terms in `window.order_by` (P-OP-2,
    /// P-OP-3, P-ORD-1).
    reads: Vec<String>,
}
#[derive(Debug)]
pub enum WindowFinding {
    ZeroOffset { path: String },
    Order { path: String, operation: String },
    RepeatedGroup { path: String, identifier: String },
}
impl WindowFinding {
    pub fn diagnostic_code(&self) -> crate::diagnostic::ConditionCode {
        use crate::diagnostic::ConditionCode as C;
        match self {
            Self::ZeroOffset { .. } => C::WindowZeroOffset,
            Self::Order { .. } => C::WindowOrderRequired,
            Self::RepeatedGroup { .. } => C::WindowRepeatedGroup,
        }
    }
    pub fn definition(&self) -> crate::diagnostic::Definition {
        self.diagnostic_code().definition()
    }
    /// Retain the authored failure without consulting a host or source table.
    /// A repeated group variable also names the form `group_by` takes
    /// (WINDOW-3).
    pub fn diagnostic(&self) -> crate::diagnostic::Diagnostic {
        use crate::diagnostic::Diagnostic;
        use crate::json::Value as JsonValue;
        let (path, name, value) = match self {
            Self::ZeroOffset { path } => (path, "offset", crate::value::Value::Int(0)),
            Self::Order { path, operation } => (
                path,
                "operation",
                crate::value::Value::Str(operation.clone()),
            ),
            Self::RepeatedGroup { path, identifier } => (
                path,
                "identifier",
                crate::value::Value::Str(identifier.clone()),
            ),
        };
        let mut context: crate::diagnostic::Context =
            [(name.into(), JsonValue::from(value))].into();
        if let Self::RepeatedGroup { .. } = self {
            context.insert("expected".into(), JsonValue::from("each variable once"));
        }
        Diagnostic {
            code: self.diagnostic_code(),
            spec_paths: vec![path.clone()],
            context,
            source_span: None,
            operand_route: None,
        }
    }
}
impl Declaration {
    /// A partition names each variable once (WINDOW-3); report every repeat.
    fn repeated_groups(&self, findings: &mut Vec<BindFinding>) {
        for (index, name) in self.groups.iter().enumerate() {
            if self.groups[..index].contains(name) {
                findings.push(BindFinding::Window(WindowFinding::RepeatedGroup {
                    path: format!("{}.window.group_by[{index}]", self.path),
                    identifier: name.clone(),
                }));
            }
        }
    }
    /// The window at `e`: its function and operands, partitions, order and
    /// filter. A baseline's date and tie-break terms are the tree's order,
    /// so the window has no order of its own; a written `window.order_by`
    /// that does not start with the date is refused before binding
    /// (WINDOW-5).
    pub fn from_tree(spec: &ast::Spec, e: ExprId) -> Result<Self, PrepareError> {
        use ast::{RankMethod, Slot, WindowFunction};
        let ExprKind::Window(window) = &spec.expr(e).kind else {
            return Err(PrepareError::Internal);
        };
        let name = |e| {
            checks::name(spec, e)
                .map(String::from)
                .ok_or(PrepareError::Internal)
        };
        let operand = |slot| checks::operand(&window.operands, slot).ok_or(PrepareError::Internal);
        let operation = window.function.verb().name();
        let (kind, order) = match &window.function {
            WindowFunction::RowNumber => (Kind::RowNumber, &window.order_by[..]),
            WindowFunction::Rank { method } => (
                match method {
                    RankMethod::Competition => Kind::Competition,
                    RankMethod::Dense => Kind::Dense,
                },
                &window.order_by[..],
            ),
            WindowFunction::RowValue { offset } => (
                Kind::RowValue {
                    source: name(operand(Slot::Source)?)?,
                    // Execution admits at most i64::MAX context rows, so every
                    // larger magnitude is an absent neighbor.
                    offset: offset.parse().unwrap_or_else(|_| {
                        if offset.starts_with('-') {
                            i64::MIN
                        } else {
                            i64::MAX
                        }
                    }),
                },
                &window.order_by[..],
            ),
            WindowFunction::Locf { include_current } => (
                Kind::Locf {
                    source: name(operand(Slot::Source)?)?,
                    include_current: *include_current,
                },
                &window.order_by[..],
            ),
            WindowFunction::BaselineFlag { inclusive, .. } => {
                let (date, tie_break) = window
                    .order_by
                    .split_first()
                    .ok_or(PrepareError::Internal)?;
                (
                    Kind::BaselineFlag {
                        date: name(date.variable)?,
                        reference_date: name(operand(Slot::Reference)?)?,
                        value: name(operand(Slot::Source)?)?,
                        inclusive: *inclusive,
                        tie_break: tie_break
                            .iter()
                            .map(|term| tree_order_term(spec, term))
                            .collect::<Result<_, _>>()?,
                    },
                    &[][..],
                )
            }
        };
        let filter = window
            .filter
            .map(|filter| match &spec.expr(filter).kind {
                ExprKind::Formula(formula) => {
                    Ok((formula.text.text.clone(), formula.compiled.clone()))
                }
                _ => Err(PrepareError::Internal),
            })
            .transpose()?;
        let at = |e: ExprId| spec.cite(spec.expr(e).meta);
        let mut reads = Vec::new();
        match &kind {
            Kind::RowValue { .. } | Kind::Locf { .. } => reads.push(at(operand(Slot::Source)?)),
            Kind::BaselineFlag { .. } => {
                let (date, tie_break) = window
                    .order_by
                    .split_first()
                    .ok_or(PrepareError::Internal)?;
                reads.push(spec.cite(date.meta));
                reads.push(at(operand(Slot::Reference)?));
                reads.push(at(operand(Slot::Source)?));
                reads.extend(tie_break.iter().map(|term| spec.cite(term.meta)));
            }
            Kind::RowNumber | Kind::Competition | Kind::Dense => (),
        }
        reads.extend(window.group_by.iter().map(|&group| at(group)));
        reads.extend(order.iter().map(|term| spec.cite(term.meta)));
        Ok(Self {
            kind,
            operation: operation.into(),
            path: checks::path(spec, e),
            groups: window
                .group_by
                .iter()
                .map(|&group| name(group))
                .collect::<Result<_, _>>()?,
            order: order
                .iter()
                .map(|term| tree_order_term(spec, term))
                .collect::<Result<_, _>>()?,
            filter,
            reads,
        })
    }
    /// Each variable the window names, with the path of the operand that
    /// names it, in authored order: the operation's own operands, then every
    /// `window.group_by` and `window.order_by` entry, each at its read's path.
    fn operands(&self) -> Vec<(&str, String)> {
        let mut names = Vec::new();
        match &self.kind {
            Kind::RowValue { source, .. } | Kind::Locf { source, .. } => {
                names.push(source.as_str())
            }
            Kind::BaselineFlag {
                date,
                reference_date,
                value,
                tie_break,
                ..
            } => {
                names.extend([date.as_str(), reference_date.as_str(), value.as_str()]);
                names.extend(tie_break.iter().map(|(name, ..)| name.as_str()));
            }
            Kind::RowNumber | Kind::Competition | Kind::Dense => (),
        }
        names.extend(self.groups.iter().map(String::as_str));
        names.extend(self.order.iter().map(|(name, ..)| name.as_str()));
        debug_assert_eq!(names.len(), self.reads.len());
        names.into_iter().zip(self.reads.iter().cloned()).collect()
    }
    /// WINDOW-11: a column-level window spans the rows of every row template
    /// and reads a qualified field at the record that built each row. When
    /// the row templates do not all share one driver, a field of a dataset
    /// whose records build some of those rows (`records`: each driver, and
    /// the input each driver intermediate reads) names no record of the
    /// other templates' rows. Each such read is refused at its operand,
    /// `window.filter` names included, naming the distinct `drivers` in
    /// construction order. Nothing here depends on a source schema.
    fn driver_findings(
        &self,
        drivers: &[&str],
        records: &[&str],
        findings: &mut Vec<crate::diagnostic::Diagnostic>,
    ) {
        if drivers.len() < 2 {
            return;
        }
        let filter_path = format!("{}.window.filter", self.path);
        let filtered = self
            .filter
            .as_ref()
            .and_then(|(_, compiled)| compiled.as_ref().ok())
            .into_iter()
            .flat_map(|plan| plan.identifiers())
            .map(|name| (name, filter_path.clone()));
        for (name, path) in self.operands().into_iter().chain(filtered) {
            if !name
                .split_once('.')
                .is_some_and(|(qualifier, _)| records.contains(&qualifier))
            {
                continue;
            }
            let mut finding = LookupFinding::new(
                &path,
                crate::diagnostic::ConditionCode::WindowDriverReference,
            );
            finding.text("identifier", name);
            finding.texts("drivers", drivers);
            finding.text("operation", &self.operation);
            findings.push(finding.diagnostic());
        }
    }
    /// A driver field the window reads is a scalar source of each row it
    /// partitions or orders. Under grouped construction it must be a group key
    /// (ROW-5 for a row template, NAME-4 at column level). A column-level
    /// window spans every constructed row, and preparation has already refused
    /// its read of a driver that some row template does not share
    /// (`column_driver_findings`, WINDOW-11). So every grouped template a
    /// column-level read reaches here is over the read's own driver, as
    /// NAME-4 checks for any column-level read. Returns whether any such read
    /// was refused. `check` reports the same refusals without a source schema,
    /// from the tree.
    pub(super) fn group_findings(
        &self,
        catalog: &Catalog,
        driver: &str,
        row: Option<&str>,
        phase: crate::reference_scope::Phase<'_>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<bool, BindError> {
        use crate::reference_scope::{self, Finding, Reach, Scope};
        let start = findings.len();
        for (name, path) in self.operands() {
            if !name.contains('.') {
                continue;
            }
            for finding in reference_scope::validate(
                catalog,
                name,
                None,
                Scope {
                    drivers: &[driver],
                    current_driver: false,
                    reach: Reach::Scalar,
                    joined: false,
                    phase,
                },
            )
            .map_err(BindError::Catalog)?
            {
                // Unknown and foreign fields keep the binder's own findings.
                if matches!(finding, Finding::RowGroup | Finding::ColumnGroup) {
                    findings.push(BindFinding::QualifiedReference {
                        path: path.clone(),
                        name: name.into(),
                        row: row.map(String::from),
                        finding,
                    });
                }
            }
        }
        Ok(findings.len() != start)
    }
    pub(super) fn bind_available(
        &self,
        catalog: &Catalog,
        row: Option<&str>,
        available: Option<&[usize]>,
        edges: &mut Vec<usize>,
        findings: &mut Vec<BindFinding>,
    ) -> Result<Option<crate::dataset::Expression>, BindError> {
        let start = findings.len();
        if matches!(self.kind, Kind::RowValue { offset: 0, .. }) {
            findings.push(BindFinding::Window(WindowFinding::ZeroOffset {
                path: format!("{}.offset", self.path),
            }));
        }
        // A positional window needs an order; a baseline orders by its date
        // and tie-break terms (WINDOW-5).
        if self.order.is_empty() && !matches!(self.kind, Kind::BaselineFlag { .. }) {
            findings.push(BindFinding::Window(WindowFinding::Order {
                path: format!("{}.window", self.path),
                operation: self.operation.clone(),
            }));
        }
        self.repeated_groups(findings);
        let filter = match &self.filter {
            Some((
                text,
                Err(CompileError::Invalid {
                    position, failure, ..
                }),
            )) => {
                findings.push(BindFinding::Lookup(LookupFinding::grammar(
                    &format!("{}.window.filter", self.path),
                    text,
                    position.character,
                    failure,
                )));
                None
            }
            Some((_, Err(error))) => return Err(BindError::ExpressionPolicy(error.clone())),
            Some((_, Ok(plan))) => Some(plan),
            None => None,
        };
        let mut sources = Vec::new();
        let mut resolve = |name: &str, path: String| -> Result<usize, BindError> {
            if name.contains('.') {
                match catalog.bind(name) {
                    Some(reference_binding::Binding::Dataset {
                        dataset: 0, field, ..
                    }) => {
                        let position =
                            match sources.iter().position(|(existing, _)| existing == name) {
                                Some(p) => p,
                                None => {
                                    sources.push((name.into(), field));
                                    sources.len() - 1
                                }
                            };
                        return Ok(catalog.output_width() + position);
                    }
                    _ => {
                        findings.push(BindFinding::UnknownReference {
                            path,
                            name: name.into(),
                        });
                        return Ok(0);
                    }
                }
            }
            if let Some(finding) = catalog
                .validate_output(name, None, available, &[0])
                .map_err(BindError::Catalog)?
            {
                findings.push(BindFinding::OutputReference {
                    path,
                    name: name.into(),
                    row: row.map(String::from),
                    finding,
                });
                return Ok(0);
            }
            let Some(reference_binding::Binding::Output { column, .. }) = catalog.bind(name) else {
                return Err(BindError::Internal);
            };
            edges.push(column);
            Ok(column)
        };
        // Each read binds at the path its tree writes, in the order of
        // `operands`: a 1.0 tree names baseline_flag's operands `reference`
        // and `source` and writes its tie-break terms in `window.order_by`
        // (P-OP-2, P-OP-3, P-ORD-1).
        let mut paths = self.operands().into_iter().map(|(_, path)| path);
        let mut path = || paths.next().ok_or(BindError::Internal);
        let kind = match &self.kind {
            Kind::RowNumber => WindowKind::RowNumber,
            Kind::Competition => WindowKind::Competition,
            Kind::Dense => WindowKind::Dense,
            Kind::RowValue { source, offset } => WindowKind::RowValue {
                column: resolve(source, path()?)?,
                offset: *offset,
            },
            Kind::Locf {
                source,
                include_current,
            } => WindowKind::Locf {
                column: resolve(source, path()?)?,
                include_current: *include_current,
            },
            Kind::BaselineFlag {
                date,
                reference_date,
                value,
                inclusive,
                ..
            } => WindowKind::BaselineFlag {
                date: resolve(date, path()?)?,
                reference_date: resolve(reference_date, path()?)?,
                value: resolve(value, path()?)?,
                inclusive: *inclusive,
            },
        };
        // The baseline's tie-break terms bind in authored order and become the
        // plan's order terms (WINDOW-9).
        let mut tie_break = Vec::new();
        if let Kind::BaselineFlag {
            tie_break: terms, ..
        } = &self.kind
        {
            for (name, descending, nulls_first) in terms {
                tie_break.push(OrderTerm {
                    column: resolve(name, path()?)?,
                    descending: *descending,
                    nulls_first: *nulls_first,
                });
            }
        }
        let mut group_by = Vec::new();
        for name in &self.groups {
            group_by.push(resolve(name, path()?)?);
        }
        let mut order_by = Vec::new();
        for (name, descending, nulls_first) in &self.order {
            order_by.push(OrderTerm {
                column: resolve(name, path()?)?,
                descending: *descending,
                nulls_first: *nulls_first,
            });
        }
        order_by.extend(tie_break);
        let mut bindings = Vec::new();
        if let Some(plan) = filter {
            let names: BTreeSet<_> = plan.identifiers().collect();
            for name in names {
                bindings.push(Binding {
                    name: name.into(),
                    read: Read::Column(resolve(name, plan.spec_path().into())?),
                });
            }
        }
        if findings.len() != start {
            return Ok(None);
        }
        if let Kind::BaselineFlag {
            date,
            reference_date,
            ..
        } = &self.kind
        {
            let kind = |name: &str| {
                catalog
                    .bind(name)
                    .map(|_| catalog.result_type(name))
                    .ok_or(BindError::Internal)
            };
            if let Some(finding) = self.baseline_type_finding(kind(date)?, kind(reference_date)?) {
                findings.push(BindFinding::Scalar(finding));
                return Ok(None);
            }
        }
        // Every two values of each order term compare (VALUE-22).
        for (name, path) in self.order_terms() {
            let kind = catalog.result_type(name);
            if kind.unordered() {
                findings.push(BindFinding::Scalar(
                    crate::diagnostic::lookup::order_term_type(
                        &path,
                        crate::diagnostic::lookup::Owner::Window(&self.operation),
                        name,
                        kind,
                    ),
                ));
            }
        }
        if findings.len() != start {
            return Ok(None);
        }
        let filter = filter
            .map(|plan| {
                let typed = plan
                    .typed(|name| crate::result_type::ResultType::declared(catalog.type_of(name)));
                BoundPredicate::new(typed, bindings)
            })
            .transpose()
            .map_err(BindError::InvalidPredicateBinding)?;
        let window = Window {
            kind,
            group_by,
            order_by,
            filter,
        };
        Ok(Some(if sources.is_empty() {
            crate::dataset::Expression::Window(window)
        } else {
            crate::dataset::Expression::ContextWindow(crate::dataset::ContextWindow {
                window,
                sources,
            })
        }))
    }
}

/// WINDOW-11 for every window of one column-level derivation: the
/// derivation itself or a `case` result, the only places a window nests
/// (EXPR-3). `drivers` are the row templates' distinct drivers in
/// construction order, and `records` each dataset whose records build rows.
pub(super) fn column_driver_findings(
    spec: &ast::Spec,
    root: ExprId,
    drivers: &[&str],
    records: &[&str],
    findings: &mut Vec<crate::diagnostic::Diagnostic>,
) -> Result<(), PrepareError> {
    for e in checks::positions(spec, root) {
        if matches!(spec.expr(e).kind, ExprKind::Window(_)) {
            Declaration::from_tree(spec, e)?.driver_findings(drivers, records, findings);
        }
    }
    Ok(())
}

/// Admit the window at `e`: its tie-break terms, partitions and order
/// within their limits, and its filter within the expression compiler's.
pub(super) fn admit(spec: &ast::Spec, e: ExprId) -> Result<(), PrepareError> {
    use crate::limits::Limits;
    let ExprKind::Window(window) = &spec.expr(e).kind else {
        return Err(PrepareError::Internal);
    };
    let path = checks::path(spec, e);
    let (tie_break, order) = match &window.function {
        ast::WindowFunction::BaselineFlag { .. } => (
            window.order_by.len().saturating_sub(1),
            window.window_order_by.len(),
        ),
        _ => (0, window.order_by.len()),
    };
    if tie_break > Limits::COLUMNS {
        return Err(PrepareError::limit(
            "window_order",
            Limits::COLUMNS,
            Some(tie_break),
            Some(&format!("{path}.tie_break")),
        ));
    }
    if window.group_by.len() > Limits::KEYS {
        return Err(PrepareError::limit(
            "window_groups",
            Limits::KEYS,
            Some(window.group_by.len()),
            Some(&format!("{path}.window.group_by")),
        ));
    }
    if order > Limits::COLUMNS {
        return Err(PrepareError::limit(
            "window_order",
            Limits::COLUMNS,
            Some(order),
            Some(&format!("{path}.window.order_by")),
        ));
    }
    derivations::predicate_limits(spec, window.filter)
}

impl Declaration {
    /// Each order term the window reads, with its read's path, in order: a
    /// baseline's date and tie-break terms, its own operands
    /// ([`Self::operands`]), or each `window.order_by` term.
    fn order_terms(&self) -> Vec<(&str, String)> {
        let mut operands = self.operands();
        let order = operands.split_off(operands.len() - self.order.len());
        match &self.kind {
            Kind::BaselineFlag { tie_break, .. } => {
                let date = operands.remove(0);
                // The reference and the source follow the date.
                std::iter::once(date)
                    .chain(operands.drain(2..2 + tie_break.len()))
                    .collect()
            }
            _ => order,
        }
    }
    /// A baseline's date has one type, `date` or `datetime`, and its
    /// reference that type (VALUE-2). A date whose types do not all compare,
    /// such as `date` beside `datetime`, is refused as an order term
    /// instead (VALUE-22); a type no source decides, or a result that is
    /// always missing, is not compared.
    fn baseline_type_finding(
        &self,
        date_kind: ResultType,
        reference_kind: ResultType,
    ) -> Option<crate::diagnostic::Diagnostic> {
        let decided = |kind: ResultType| kind != ResultType::UNKNOWN && !kind.is_missing();
        if !decided(date_kind) || date_kind.unordered() {
            return None;
        }
        let Kind::BaselineFlag {
            date,
            reference_date,
            ..
        } = &self.kind
        else {
            return None;
        };
        let invalid = match date_kind.column_type() {
            Some(ColumnType::Date | ColumnType::DateTime) => {
                (decided(reference_kind) && reference_kind != date_kind).then(|| {
                    (
                        "reference_date",
                        reference_date,
                        date_kind.name(),
                        reference_kind,
                    )
                })
            }
            _ => Some(("date", date, "date or datetime".into(), date_kind)),
        };
        let (operand, source, expected, actual) = invalid?;
        // The date is the first read and the reference the second.
        let path = match operand {
            "date" => self.reads.first()?.clone(),
            _ => self.reads.get(1)?.clone(),
        };
        let mut finding = LookupFinding::new(
            &path,
            crate::diagnostic::ConditionCode::WindowBaselineInputType,
        );
        for (name, value) in [
            ("operation", Verb::BaselineFlag.name()),
            ("source", source.as_str()),
            ("expected", expected.as_str()),
            ("actual", actual.name().as_str()),
        ] {
            finding.text(name, value);
        }
        Some(finding.diagnostic())
    }
}
