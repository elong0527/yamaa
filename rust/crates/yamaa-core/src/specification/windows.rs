//! Compile original output-window declarations into the existing shared evaluator.
use super::*;
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
    /// The path of each read, in [`Declaration::operands`] order, as a 1.0
    /// tree writes it: `baseline_flag` names its operands `source` and
    /// `reference` and its tie-break terms in `window.order_by` (P-OP-2,
    /// P-OP-3, P-ORD-1). A 0.5 tree's reads compose the field names below.
    surface_reads: Option<Vec<String>>,
}
#[derive(Debug)]
pub enum WindowFinding {
    ZeroOffset {
        path: String,
    },
    Order {
        path: String,
        operation: String,
        forbidden: bool,
    },
    RepeatedGroup {
        path: String,
        identifier: String,
    },
}
impl WindowFinding {
    pub fn diagnostic_code(&self) -> crate::diagnostic::ConditionCode {
        use crate::diagnostic::ConditionCode as C;
        match self {
            Self::ZeroOffset { .. } => C::WindowZeroOffset,
            Self::Order {
                forbidden: false, ..
            } => C::WindowOrderRequired,
            Self::Order {
                forbidden: true, ..
            } => C::WindowOrderForbidden,
            Self::RepeatedGroup { .. } => C::WindowRepeatedGroup,
        }
    }
    pub fn definition(&self) -> crate::diagnostic::Definition {
        self.diagnostic_code().definition()
    }
    /// Retain the authored failure without consulting a host or source table.
    pub fn diagnostic(&self) -> crate::diagnostic::Diagnostic {
        use crate::diagnostic::Diagnostic;
        use crate::json::Value as JsonValue;
        let (path, name, value) = match self {
            Self::ZeroOffset { path } => (path, "offset", crate::value::Value::Int(0)),
            Self::Order {
                path, operation, ..
            } => (
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
        Diagnostic {
            code: self.diagnostic_code(),
            spec_paths: vec![path.clone()],
            context: [(name.into(), JsonValue::from(value))].into(),
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
    /// filter. A baseline's date and tie-break terms are the tree's order;
    /// its written `window.order_by`, which is refused, is the window's own.
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
        let (kind, operation, order) = match &window.function {
            WindowFunction::RowNumber => (Kind::RowNumber, "row_number", &window.order_by[..]),
            WindowFunction::Rank { method } => (
                match method {
                    RankMethod::Competition => Kind::Competition,
                    RankMethod::Dense => Kind::Dense,
                },
                "rank",
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
                "row_value",
                &window.order_by[..],
            ),
            WindowFunction::Locf { include_current } => (
                Kind::Locf {
                    source: name(operand(Slot::Source)?)?,
                    include_current: *include_current,
                },
                "locf",
                &window.order_by[..],
            ),
            WindowFunction::BaselineFlag {
                inclusive,
                window_order_by,
            } => {
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
                    "baseline_flag",
                    &window_order_by[..],
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
        let surface_reads = match spec.header.surface {
            ast::Surface::V0_5 => None,
            ast::Surface::V1 { .. } => {
                let at = |e: ExprId| spec.cite(spec.expr(e).meta);
                let mut reads = Vec::new();
                match &kind {
                    Kind::RowValue { .. } | Kind::Locf { .. } => {
                        reads.push(at(operand(Slot::Source)?))
                    }
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
                Some(reads)
            }
        };
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
            surface_reads,
        })
    }
    /// Each variable the window names, with the path of the operand that
    /// names it, in authored order: the operation's own operands, then every
    /// `window.group_by` and `window.order_by` entry.
    fn operands(&self) -> Vec<(&str, String)> {
        let mut reads = Vec::new();
        match &self.kind {
            Kind::RowValue { source, .. } | Kind::Locf { source, .. } => {
                reads.push((source.as_str(), format!("{}.source", self.path)))
            }
            Kind::BaselineFlag {
                date,
                reference_date,
                value,
                tie_break,
                ..
            } => {
                reads.push((date.as_str(), format!("{}.date", self.path)));
                reads.push((
                    reference_date.as_str(),
                    format!("{}.reference_date", self.path),
                ));
                reads.push((value.as_str(), format!("{}.value", self.path)));
                for (index, (name, ..)) in tie_break.iter().enumerate() {
                    reads.push((name.as_str(), format!("{}.tie_break[{index}]", self.path)));
                }
            }
            Kind::RowNumber | Kind::Competition | Kind::Dense => (),
        }
        for (index, name) in self.groups.iter().enumerate() {
            reads.push((
                name.as_str(),
                format!("{}.window.group_by[{index}]", self.path),
            ));
        }
        for (index, (name, ..)) in self.order.iter().enumerate() {
            reads.push((
                name.as_str(),
                format!("{}.window.order_by[{index}]", self.path),
            ));
        }
        if let Some(paths) = &self.surface_reads {
            debug_assert_eq!(paths.len(), reads.len());
            for ((_, path), surface) in reads.iter_mut().zip(paths) {
                path.clone_from(surface);
            }
        }
        reads
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
        let baseline = matches!(self.kind, Kind::BaselineFlag { .. });
        if self.order.is_empty() != baseline {
            findings.push(BindFinding::Window(WindowFinding::Order {
                path: format!(
                    "{}.window{}",
                    self.path,
                    if baseline { ".order_by" } else { "" }
                ),
                operation: self.operation.clone(),
                forbidden: baseline,
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
        let kind = match &self.kind {
            Kind::RowNumber => WindowKind::RowNumber,
            Kind::Competition => WindowKind::Competition,
            Kind::Dense => WindowKind::Dense,
            Kind::RowValue { source, offset } => WindowKind::RowValue {
                column: resolve(source, format!("{}.source", self.path))?,
                offset: *offset,
            },
            Kind::Locf {
                source,
                include_current,
            } => WindowKind::Locf {
                column: resolve(source, format!("{}.source", self.path))?,
                include_current: *include_current,
            },
            Kind::BaselineFlag {
                date,
                reference_date,
                value,
                inclusive,
                ..
            } => WindowKind::BaselineFlag {
                date: resolve(date, format!("{}.date", self.path))?,
                reference_date: resolve(reference_date, format!("{}.reference_date", self.path))?,
                value: resolve(value, format!("{}.value", self.path))?,
                inclusive: *inclusive,
            },
        };
        // The baseline's tie-break terms bind in authored order and become the
        // plan's order terms; `window.order_by` stays forbidden beside them.
        let mut tie_break = Vec::new();
        if let Kind::BaselineFlag {
            tie_break: terms, ..
        } = &self.kind
        {
            for (index, (name, descending, nulls_first)) in terms.iter().enumerate() {
                tie_break.push(OrderTerm {
                    column: resolve(name, format!("{}.tie_break[{index}]", self.path))?,
                    descending: *descending,
                    nulls_first: *nulls_first,
                });
            }
        }
        let mut group_by = Vec::new();
        for (index, name) in self.groups.iter().enumerate() {
            group_by.push(resolve(
                name,
                format!("{}.window.group_by[{index}]", self.path),
            )?);
        }
        let mut order_by = Vec::new();
        for (index, (name, descending, nulls_first)) in self.order.iter().enumerate() {
            order_by.push(OrderTerm {
                column: resolve(name, format!("{}.window.order_by[{index}]", self.path))?,
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
            let kind = |name: &str| match catalog.bind(name) {
                Some(reference_binding::Binding::Output { column_type, .. })
                | Some(reference_binding::Binding::Dataset { column_type, .. }) => Ok(column_type),
                None => Err(BindError::Internal),
            };
            if let Some(finding) = self.baseline_type_finding(kind(date)?, kind(reference_date)?) {
                findings.push(BindFinding::Scalar(finding));
                return Ok(None);
            }
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
        ast::WindowFunction::BaselineFlag {
            window_order_by, ..
        } => (
            window.order_by.len().saturating_sub(1),
            window_order_by.len(),
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

fn type_name(kind: ColumnType) -> &'static str {
    match kind {
        ColumnType::Str => "str",
        ColumnType::Int => "int",
        ColumnType::Float => "float",
        ColumnType::Date => "date",
        ColumnType::DateTime => "datetime",
    }
}

impl Declaration {
    fn baseline_type_finding(
        &self,
        date_kind: ColumnType,
        reference_kind: ColumnType,
    ) -> Option<crate::diagnostic::Diagnostic> {
        let Kind::BaselineFlag {
            date,
            reference_date,
            ..
        } = &self.kind
        else {
            return None;
        };
        let invalid = if !matches!(date_kind, ColumnType::Date | ColumnType::DateTime) {
            Some(("date", date, "date or datetime", date_kind))
        } else if date_kind != reference_kind {
            Some((
                "reference_date",
                reference_date,
                type_name(date_kind),
                reference_kind,
            ))
        } else {
            None
        };
        let (operand, source, expected, actual) = invalid?;
        // A 1.0 tree's date is its first read and its reference the second.
        let path = match (&self.surface_reads, operand) {
            (Some(paths), "date") => paths.first()?.clone(),
            (Some(paths), _) => paths.get(1)?.clone(),
            (None, _) => format!("{}.{operand}", self.path),
        };
        let mut finding = LookupFinding::new(
            &path,
            crate::diagnostic::ConditionCode::WindowBaselineInputType,
        );
        for (name, value) in [
            ("operation", "baseline_flag"),
            ("source", source.as_str()),
            ("expected", expected),
            ("actual", type_name(actual)),
        ] {
            finding.text(name, value);
        }
        Some(finding.diagnostic())
    }
}
