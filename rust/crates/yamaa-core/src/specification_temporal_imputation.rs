//! Original imputation declarations retain literal handlers and named operand scope.
use super::*;
use crate::{
    dataset::{date_policy, DatePolicyError, DayDeclaration, Imputation, ImputationKind, TimeRule},
    temporal::DatePrecision,
};
const OPERATIONS: &[&str] = &[
    "date_impute",
    "date_precision",
    "datetime_impute",
    "datetime_precision",
    "to_date",
];
fn integer(d: &Document, id: usize) -> Result<&str, PrepareError> {
    match &d.nodes()[id] {
        N::Integer(value) => Ok(value),
        _ => Err(PrepareError::Internal),
    }
}
pub(super) fn prepare(
    d: &Document,
    payload: usize,
    op: &str,
    path: &str,
    driver: Option<&str>,
    intermediates: Option<&intermediates::Declarations>,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<Imputation, PrepareError> {
    let allowed: &[&str] = match op {
        "date_impute" => &[
            "source",
            "month",
            "day",
            "minimum_source_precision",
            "not_before",
            "missing",
            "invalid",
        ],
        "datetime_impute" => &["source", "time", "missing", "invalid"],
        "to_date" => &["source"],
        _ => &["source", "missing", "invalid"],
    };
    for &(key, _) in mapping(d, payload)? {
        let name = text(d, key)?;
        if !allowed.contains(&name) {
            reject(extra, name, format!("{path}.{name}"));
        }
    }
    let source = text(d, field(d, payload, "source")?)?;
    let bound = d
        .field(payload, "not_before")
        .map(|id| text(d, id))
        .transpose()?;
    let handler_text = |name: &str| {
        d.field(payload, name).and_then(|id| match &d.nodes()[id] {
            N::Text(text) => Some(text.as_str()),
            _ => None,
        })
    };
    if !Imputation::metadata_fits(
        source,
        bound,
        path,
        handler_text("missing"),
        handler_text("invalid"),
    ) {
        return Err(PrepareError::Limit("imputation_metadata"));
    }
    for (field_name, name) in
        core::iter::once(("source", source)).chain(bound.map(|name| ("not_before", name)))
    {
        if name.split_once('.').is_some_and(|(relation, _)| {
            driver.is_some_and(|driver| relation != driver)
                && !intermediates.is_some_and(|items| items.contains(relation))
        }) {
            reject(
                extra,
                "secondary_source_expression",
                format!("{path}.{field_name}"),
            );
        }
    }
    let kind = match op {
        "date_impute" => {
            let minimum = match d
                .field(payload, "minimum_source_precision")
                .map(|id| text(d, id))
                .transpose()?
                .unwrap_or("year")
            {
                "year" => DatePrecision::Year,
                "month" => DatePrecision::Month,
                _ => return Err(PrepareError::Internal),
            };
            let month = d
                .field(payload, "month")
                .map(|id| integer(d, id))
                .transpose()?;
            let day = match &d.nodes()[field(d, payload, "day")?] {
                N::Text(value) => DayDeclaration::Position(value),
                N::Integer(value) => DayDeclaration::Number(value),
                _ => return Err(PrepareError::Internal),
            };
            date_policy(month, day, minimum, path).map_err(|error| match error {
                DatePolicyError::Diagnostic(diagnostic) => {
                    PrepareError::Invalid(vec![PreflightFinding::Imputation(diagnostic)])
                }
                DatePolicyError::Limit => PrepareError::Limit("imputation_metadata"),
                DatePolicyError::Shape => PrepareError::Internal,
            })?
        }
        "date_precision" => ImputationKind::DatePrecision,
        "datetime_impute" => ImputationKind::DateTime {
            time: match text(d, field(d, payload, "time")?)? {
                "first" => TimeRule::First,
                "last" => TimeRule::Last,
                _ => return Err(PrepareError::Internal),
            },
        },
        "datetime_precision" => ImputationKind::DateTimePrecision,
        "to_date" => ImputationKind::ToDate,
        _ => return Err(PrepareError::Internal),
    };
    let handler = |name: &str| {
        d.field(payload, name)
            .map(|id| literal(d, id, &format!("{path}.{name}")))
            .transpose()
    };
    Imputation::new(
        kind,
        source.into(),
        bound.map(String::from),
        path.into(),
        handler("missing")?,
        handler("invalid")?,
    )
    .ok_or(PrepareError::Internal)
}
/// Original known output kinds and completion policy fail before source authority.
pub(super) fn declaration_findings(
    d: &Document,
    columns: &[usize],
    output: &TableSchema,
    findings: &mut Vec<PreflightFinding>,
) -> Result<(), PrepareError> {
    #[allow(clippy::too_many_arguments)]
    fn visit(
        d: &Document,
        value: usize,
        path: &str,
        output: &TableSchema,
        findings: &mut Vec<PreflightFinding>,
        depth: usize,
        rows: bool,
        bytes: &mut usize,
    ) -> Result<(), PrepareError> {
        if depth > 64 || path.len() > 65_536 {
            return Err(PrepareError::Limit("case_depth"));
        }
        for &op in OPERATIONS {
            if let Some(payload) = d.field(value, op) {
                let path = format!("{path}.{op}");
                // Charge occurrence copies before policy/context construction, even
                // if an admitted arena shares one long scalar across declarations.
                *bytes = bytes
                    .checked_add(path.len())
                    .and_then(|n| n.checked_add(512))
                    .filter(|n| *n <= 16_777_216)
                    .ok_or(PrepareError::Limit("imputation_metadata"))?;
                for &(key, value) in mapping(d, payload)? {
                    for id in [key, value] {
                        if let N::Text(text) | N::Integer(text) = &d.nodes()[id] {
                            *bytes = bytes
                                .checked_add(text.len())
                                .filter(|n| *n <= 16_777_216)
                                .ok_or(PrepareError::Limit("imputation_metadata"))?;
                        }
                    }
                }
                match prepare(d, payload, op, &path, None, None, &mut Vec::new()) {
                    Ok(operation) => {
                        let kind = if rows {
                            None
                        } else {
                            output
                                .columns()
                                .iter()
                                .find(|item| item.name == operation.source())
                                .map(|item| item.kind)
                        }
                        .or(temporal::declared_source_type(d, operation.source())?);
                        if let Some(kind) = kind {
                            if !operation.accepts(kind) {
                                let actual = match kind {
                                    ColumnType::Str => "str",
                                    ColumnType::Int => "int",
                                    ColumnType::Float => "float",
                                    ColumnType::Date => "date",
                                    ColumnType::DateTime => "datetime",
                                };
                                findings.push(PreflightFinding::Imputation(
                                    operation.input_diagnostic(actual),
                                ));
                            }
                        }
                        if let Some(bound) = operation.bound_source() {
                            let kind = if rows {
                                None
                            } else {
                                output
                                    .columns()
                                    .iter()
                                    .find(|item| item.name == bound)
                                    .map(|item| item.kind)
                            }
                            .or(temporal::declared_source_type(d, bound)?);
                            if let Some(kind) = kind.filter(|kind| *kind != ColumnType::Date) {
                                let actual = match kind {
                                    ColumnType::Str => "str",
                                    ColumnType::Int => "int",
                                    ColumnType::Float => "float",
                                    ColumnType::DateTime => "datetime",
                                    ColumnType::Date => unreachable!(),
                                };
                                findings.push(PreflightFinding::Imputation(
                                    operation.bound_input_diagnostic(actual),
                                ));
                            }
                        }
                    }
                    Err(PrepareError::Invalid(errors)) => findings.extend(errors),
                    Err(error) => return Err(error),
                }
            }
        }
        if let Some(case) = d.field(value, "case") {
            for (index, &branch) in sequence(d, case)?.iter().enumerate() {
                let (name, child) = d
                    .field(branch, "then")
                    .map(|id| ("then", id))
                    .or_else(|| d.field(branch, "otherwise").map(|id| ("otherwise", id)))
                    .ok_or(PrepareError::Internal)?;
                visit(
                    d,
                    child,
                    &format!("{path}.case[{index}].{name}"),
                    output,
                    findings,
                    depth + 1,
                    rows,
                    bytes,
                )?;
            }
        }
        Ok(())
    }
    let rows = present(d, d.root(), "rows");
    let mut bytes = 0;
    for (index, &column) in columns.iter().enumerate() {
        if let Some(value) = d
            .field(column, "derivation")
            .and_then(|id| d.field(id, "value"))
        {
            visit(
                d,
                value,
                &format!("columns.{}.derivation", output.columns()[index].name),
                output,
                findings,
                0,
                rows,
                &mut bytes,
            )?;
        }
    }
    if let Some(rows) = d.field(d.root(), "rows") {
        for (index, &row) in sequence(d, rows)?.iter().enumerate() {
            if let Some(derivations) = d.field(row, "derivations") {
                for &(name, derivation) in mapping(d, derivations)? {
                    if let Some(value) = d.field(derivation, "value") {
                        visit(
                            d,
                            value,
                            &format!("rows[{index}].derivations.{}", text(d, name)?),
                            output,
                            findings,
                            0,
                            true,
                            &mut bytes,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind(
    operation: &Imputation,
    column: usize,
    keys: &[usize],
    catalog: &Catalog,
    declarations: &intermediates::Declarations,
    secondary: &[crate::dataset::SecondarySource],
    intermediates: &[Option<crate::dataset::Intermediate>],
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let mut operands = Vec::new();
    for (name, field) in core::iter::once((operation.source(), "source")).chain(
        operation
            .bound_source()
            .map(|source| (source, "not_before")),
    ) {
        let path = format!("{}.{}", operation.path(), field);
        if let Some(expression) = rounding::bind_operand(
            name,
            &path,
            column,
            keys,
            catalog,
            declarations,
            secondary,
            intermediates,
            edges,
            findings,
        )? {
            operands.push(Assignment {
                column,
                path,
                expression,
            });
        }
    }
    Ok(
        (operands.len() == 1 + usize::from(operation.bound_source().is_some())).then(|| {
            Expression::Imputation {
                operands,
                operation: operation.clone(),
            }
        }),
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_row(
    operation: &Imputation,
    column: usize,
    catalog: &Catalog,
    driver: &str,
    row: Option<&str>,
    phase: crate::reference_scope::Phase<'_>,
    available: Option<&[usize]>,
    edges: &mut Vec<usize>,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Expression>, BindError> {
    let mut operands = Vec::new();
    for (name, field) in core::iter::once((operation.source(), "source")).chain(
        operation
            .bound_source()
            .map(|source| (source, "not_before")),
    ) {
        let path = format!("{}.{}", operation.path(), field);
        if let Some(expression) = rounding::bind_row_operand(
            name, &path, catalog, driver, row, phase, available, edges, findings,
        )? {
            operands.push(Assignment {
                column,
                path,
                expression,
            });
        }
    }
    Ok(
        (operands.len() == 1 + usize::from(operation.bound_source().is_some())).then(|| {
            Expression::Imputation {
                operands,
                operation: operation.clone(),
            }
        }),
    )
}
