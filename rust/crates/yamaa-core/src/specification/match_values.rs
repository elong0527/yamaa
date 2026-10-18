//! Original key expressions bind in current-row scope, without a target column.
use super::*;
use crate::{
    dataset::{MatchRead, MatchValue},
    result_type::ResultType,
};

#[derive(Clone, Copy)]
pub(super) struct Context<'a> {
    pub output: &'a TableSchema,
    pub driver: &'a str,
    pub row: Option<&'a str>,
    pub phase: crate::reference_scope::Phase<'a>,
    pub project: Option<&'a crate::project_calls::ProjectCalls>,
    pub named: &'a intermediates::Declarations,
    pub secondary: &'a [crate::dataset::SecondarySource],
    pub intermediates: &'a [Option<crate::dataset::Intermediate>],
}
#[derive(Clone, Debug)]
pub(super) enum Declaration {
    Variable(String),
    Source(String),
    Literal(crate::value::Value),
    StrPad(str_pad::Declaration),
    Expression {
        branch: cases::Branch,
        references: Vec<String>,
    },
}
impl Declaration {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        d: &Document,
        value: usize,
        path: &str,
        _sources: &[SourceDeclaration],
        _driver: usize,
        project: Option<&functions::Prepared>,
        extraction: &str_extract::Prepared,
        templates: &str_template::Prepared,
        extra: &mut Vec<UnsupportedFeature>,
    ) -> Result<Self, PrepareError> {
        if let N::Text(name) = &d.nodes()[value] {
            return Ok(Self::Variable(name.clone()));
        }
        let &[(operation, payload)] = mapping(d, value)? else {
            return Err(PrepareError::Internal);
        };
        let op = text(d, operation)?;
        let operation_path = format!("{path}.{op}");
        let declaration = match op {
            "literal" => Self::Literal(literal(d, payload, &operation_path)?),
            "source" => {
                let name = if let N::Text(name) = &d.nodes()[payload] {
                    name.as_str()
                } else {
                    for &(key, value) in mapping(d, payload)? {
                        let key = text(d, key)?;
                        if key != "variable" && !matches!(d.nodes()[value], N::Null) {
                            reject(
                                extra,
                                "intermediate_key_source",
                                format!("{operation_path}.{key}"),
                            );
                        }
                    }
                    text(d, field(d, payload, "variable")?)?
                };
                Self::Source(name.into())
            }
            "str_pad" => Self::StrPad(str_pad::prepare(
                d,
                payload,
                &operation_path,
                None,
                None,
                extra,
            )?),
            "str_case"
            | "impute"
            | "to_date"
            | "precision"
            | "imputation_flag"
            | "round_half_away_from_zero"
            | "diff"
            | "study_day"
            | "to_epoch_day"
            | "mapping"
            | "cut"
            | "greatest"
            | "least"
            | "str_extract"
            | "str_replace"
            | "str_template"
            | "compute"
            | "row_number"
            | "rank"
            | "baseline_flag"
            | "row_value"
            | "locf"
            | "shift"
            | "duration_value"
            | "dtc"
            | "time_of_day" => {
                let mut references = Vec::new();
                derivations::references(d, value, project, &mut references, 0)?;
                self_relations::template_references(d, value, templates, &mut references, 0)?;
                let branch = derivations::prepare_value(
                    d,
                    value,
                    path,
                    project,
                    &intermediates::Declarations::default(),
                    false,
                    0,
                    extraction,
                    templates,
                )?;
                reject_deferred_owners(&branch, extra);
                Self::Expression { branch, references }
            }
            _ => {
                reject(extra, op, operation_path);
                // A rejected operation never becomes an executable plan.
                Self::Literal(crate::value::Value::Missing)
            }
        };
        if let Some(name) = declaration.expression_reference() {
            if let Some((relation, _)) = name.split_once('.') {
                if d.field(d.root(), "intermediates").is_some_and(|items| {
                    sequence(d, items).is_ok_and(|items| {
                        items.iter().any(|&item| {
                            d.field(item, "id")
                                .is_some_and(|id| text(d, id).is_ok_and(|name| name == relation))
                        })
                    })
                }) {
                    reject(extra, "intermediate_key_expression", path.into());
                }
            }
        }
        Ok(declaration)
    }
    fn expression_reference(&self) -> Option<&str> {
        match self {
            Self::Source(name) => Some(name),
            Self::StrPad(pad) => Some(pad.source()),
            _ => None,
        }
    }
    pub(super) fn reference(&self) -> Option<&str> {
        match self {
            Self::Variable(name) | Self::Source(name) => Some(name),
            Self::StrPad(pad) => Some(pad.source()),
            Self::Literal(_) | Self::Expression { .. } => None,
        }
    }
    pub(super) fn references(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        match self {
            Self::Expression { references, .. } => Box::new(references.iter().map(String::as_str)),
            _ => Box::new(self.reference().into_iter()),
        }
    }
    pub(super) fn display_name(&self, field: &str) -> String {
        match self {
            Self::Variable(name) => name.clone(),
            _ => format!("key[{field}]"),
        }
    }
    /// The match value's result type (EXPR-14): the read variable's, a
    /// literal's, `str_pad`'s text, or the expression's over the names the
    /// current row types. LOOKUP-6 compares it with the key column's type.
    pub(super) fn result_type(&self, catalog: &Catalog) -> ResultType {
        let input = |name: &str| ResultType::declared(catalog.type_of(name));
        match self {
            Self::Variable(name) | Self::Source(name) => input(name),
            Self::Literal(value) => ResultType::of_value(value),
            Self::StrPad(pad) => Operation::StrPad(pad.clone()).result_type(&input),
            Self::Expression { branch, .. } => branch.operation.result_type(&input),
        }
    }
    pub(super) fn bind(
        &self,
        path: &str,
        intermediate: &str,
        catalog: &Catalog,
        findings: &mut Vec<BindFinding>,
        context: Context<'_>,
    ) -> Result<Option<MatchValue>, BindError> {
        self.bind_in_context(path, intermediate, catalog, findings, false, context)
    }
    pub(super) fn bind_deferred(
        &self,
        path: &str,
        intermediate: &str,
        catalog: &Catalog,
        findings: &mut Vec<BindFinding>,
        context: Context<'_>,
    ) -> Result<Option<MatchValue>, BindError> {
        self.bind_in_context(path, intermediate, catalog, findings, true, context)
    }
    #[allow(clippy::too_many_arguments)]
    fn bind_in_context(
        &self,
        path: &str,
        intermediate: &str,
        catalog: &Catalog,
        findings: &mut Vec<BindFinding>,
        defer_unknown: bool,
        context: Context<'_>,
    ) -> Result<Option<MatchValue>, BindError> {
        if let Self::Expression {
            branch, references, ..
        } = self
        {
            if defer_unknown {
                for name in references {
                    if catalog.bind(name).is_none() {
                        return Ok(Some(MatchValue::Contextual(
                            crate::dataset::ContextualMatch::Expression {
                                identifier: name.clone(),
                            },
                        )));
                    }
                }
            }
            let before = findings.len();
            let mut reads = Vec::new();
            let mut fields = Vec::new();
            for name in references {
                let Some(bound) = catalog.bind(name) else {
                    findings.push(BindFinding::Lookup(LookupFinding::reference(
                        path,
                        name,
                        Some(intermediate),
                        ReferenceCause::OutputKey,
                        None,
                    )));
                    continue;
                };
                let (read, kind) = match bound {
                    reference_binding::Binding::Output {
                        column,
                        column_type,
                    } => (
                        MatchRead::Output {
                            column,
                            identifier: name.clone(),
                        },
                        column_type,
                    ),
                    reference_binding::Binding::Dataset {
                        dataset: 0,
                        field,
                        column_type,
                    } => (
                        MatchRead::Record {
                            column: field,
                            identifier: name.clone(),
                        },
                        column_type,
                    ),
                    reference_binding::Binding::Dataset {
                        dataset,
                        field,
                        column_type,
                    } => (
                        MatchRead::SecondaryRecord {
                            source: dataset - 1,
                            column: field,
                            identifier: name.clone(),
                        },
                        column_type,
                    ),
                };
                if !reads
                    .iter()
                    .any(|prior: &MatchRead| prior.identifier() == name)
                {
                    reads.push(read);
                    fields.push(Column {
                        name: name.clone(),
                        kind,
                    });
                }
            }
            if findings.len() != before {
                return Ok(None);
            }
            let mut edges = Vec::new();
            let expression = derivations::bind_row(
                &branch.operation,
                &branch.path,
                context.output.columns().len(),
                catalog,
                context.driver,
                context.row,
                context.phase,
                None,
                &mut edges,
                findings,
                false,
                context.output,
                context.project,
                context.named,
                context.secondary,
                context.intermediates,
            )?;
            return expression
                .map(|expression| {
                    crate::dataset::MatchExpression::new(
                        Assignment {
                            column: context.output.columns().len(),
                            expression,
                            path: branch.path.clone(),
                        },
                        reads,
                        TableSchema::new(fields).map_err(|_| BindError::Internal)?,
                        self.result_type(catalog),
                    )
                    .map(|expression| MatchValue::Expression(Box::new(expression)))
                    .map_err(BindError::InvalidPlan)
                })
                .transpose();
        }
        if let Self::StrPad(operation) = self {
            str_pad::declaration_findings(operation, findings);
        }
        let Some(name) = self.reference() else {
            let Self::Literal(value) = self else {
                return Err(BindError::Internal);
            };
            return Ok(Some(MatchValue::Literal(value.clone())));
        };
        let read = match catalog.bind(name) {
            Some(reference_binding::Binding::Output { column, .. }) => MatchRead::Output {
                column,
                identifier: name.into(),
            },
            Some(reference_binding::Binding::Dataset {
                dataset: 0, field, ..
            }) => MatchRead::Record {
                column: field,
                identifier: name.into(),
            },
            Some(reference_binding::Binding::Dataset { dataset, field, .. }) => {
                MatchRead::SecondaryRecord {
                    source: dataset - 1,
                    column: field,
                    identifier: name.into(),
                }
            }
            None if defer_unknown => {
                use crate::dataset::ContextualMatch;
                let declaration = match self {
                    Self::Variable(_) => ContextualMatch::Variable(name.into()),
                    Self::Source(_) => ContextualMatch::Source(name.into()),
                    Self::StrPad(operation) => ContextualMatch::StrPad {
                        identifier: name.into(),
                        operation: match operation {
                            str_pad::Declaration::Valid(operation) => Some(operation.clone()),
                            str_pad::Declaration::WidthError { .. } => None,
                        },
                    },
                    Self::Literal(_) | Self::Expression { .. } => return Err(BindError::Internal),
                };
                return Ok(Some(MatchValue::Contextual(declaration)));
            }
            None => {
                let suggestion = name
                    .strip_prefix("SELF.")
                    .filter(|bare| {
                        catalog.bind(bare).is_some_and(|item| {
                            matches!(item, reference_binding::Binding::Output { .. })
                        })
                    })
                    .map(String::from);
                findings.push(BindFinding::Lookup(LookupFinding::reference(
                    path,
                    name,
                    Some(intermediate),
                    ReferenceCause::OutputKey,
                    suggestion,
                )));
                return Ok(None);
            }
        };
        Ok(Some(match self {
            Self::Variable(_) => MatchValue::Variable(read),
            Self::Source(_) => MatchValue::Source(read),
            Self::StrPad(operation) => MatchValue::StrPad {
                read,
                operation: match operation {
                    str_pad::Declaration::Valid(operation) => Some(operation.clone()),
                    str_pad::Declaration::WidthError { .. } => None,
                },
            },
            Self::Literal(_) | Self::Expression { .. } => return Err(BindError::Internal),
        }))
    }
}

/// Conditional and callback relation contexts need a demand-aware key owner.
/// Keep them outside this bounded scalar/window expansion rather than
/// pre-reading their unselected branches.
fn reject_deferred_owners(branch: &cases::Branch, extra: &mut Vec<UnsupportedFeature>) {
    let operation = match branch.operation.as_ref() {
        Operation::Case(_) => Some("case"),
        Operation::ProjectFunction { .. } => Some("function"),
        Operation::Aggregate(_) => Some("aggregate"),
        Operation::Odm(_) => Some("odm"),
        Operation::Flag(_) => Some("flag"),
        Operation::FirstAvailable(_) => Some("first_available"),
        Operation::Imputation(operation) if operation.has_conditional_operands() => {
            for (field, _) in operation.operands().skip(1) {
                reject(extra, field, format!("{}.{field}", branch.path));
            }
            None
        }
        _ => None,
    };
    if let Some(operation) = operation {
        reject(extra, operation, branch.path.clone());
    }
}

/// Reject aggregate key counts before compiling their authored expressions.
pub(super) fn declaration_admission(d: &Document) -> Result<(), PrepareError> {
    let Some(items) = d
        .field(d.root(), "intermediates")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    else {
        return Ok(());
    };
    let items = sequence(d, items)?;
    if items.len() > crate::limits::Limits::COLUMNS {
        return Err(PrepareError::limit(
            "intermediates",
            crate::limits::Limits::COLUMNS,
            Some(items.len()),
            None,
        ));
    }
    for (index, &item) in items.iter().enumerate() {
        if let Some(key) = d.field(item, "key") {
            if let N::Mapping(fields) = &d.nodes()[key] {
                if fields.len() > crate::limits::Limits::KEYS {
                    return Err(PrepareError::limit(
                        "intermediate_keys",
                        crate::limits::Limits::KEYS,
                        Some(fields.len()),
                        Some(&format!("intermediates[{index}].key")),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Share authored key occurrence paths with the existing declaration validators.
/// Each operation owner still decides which input types and metadata to inspect.
pub(super) fn visit_keys(
    d: &Document,
    visit: &mut impl FnMut(usize, &str) -> Result<(), PrepareError>,
) -> Result<(), PrepareError> {
    fn key(
        d: &Document,
        owner: usize,
        path: &str,
        visit: &mut impl FnMut(usize, &str) -> Result<(), PrepareError>,
    ) -> Result<(), PrepareError> {
        if let Some(id) = d.field(owner, "key") {
            if let N::Mapping(pairs) = &d.nodes()[id] {
                for &(name, value) in pairs {
                    if !matches!(d.nodes()[value], N::Text(_)) {
                        visit(value, &format!("{path}.key.{}", text(d, name)?))?;
                    }
                }
            }
        }
        Ok(())
    }
    fn expression(
        d: &Document,
        value: usize,
        path: &str,
        depth: usize,
        visit: &mut impl FnMut(usize, &str) -> Result<(), PrepareError>,
    ) -> Result<(), PrepareError> {
        if depth > crate::limits::Limits::SEMANTIC_DEPTH {
            return Err(PrepareError::limit(
                "semantic_depth",
                crate::limits::Limits::SEMANTIC_DEPTH,
                Some(depth),
                Some(path),
            ));
        }

        if let Some(payload) = d.field(value, "aggregate") {
            key(d, payload, &format!("{path}.aggregate"), visit)?;
        }
        if let Some(case) = d.field(value, "case") {
            for (index, &branch) in sequence(d, case)?.iter().enumerate() {
                for name in ["then", "otherwise"] {
                    if let Some(value) = d.field(branch, name) {
                        expression(
                            d,
                            value,
                            &format!("{path}.case[{index}].{name}"),
                            depth + 1,
                            visit,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
    for &column in sequence(d, field(d, d.root(), "columns")?)? {
        if let Some(value) = d
            .field(column, "derivation")
            .and_then(|id| d.field(id, "value"))
        {
            expression(
                d,
                value,
                &format!("columns.{}.derivation", text(d, field(d, column, "name")?)?),
                0,
                visit,
            )?;
        }
    }
    if let Some(rows) = d
        .field(d.root(), "rows")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    {
        for (index, &row) in sequence(d, rows)?.iter().enumerate() {
            if let Some(derivations) = d.field(row, "derivations") {
                for &(name, handled) in mapping(d, derivations)? {
                    if let Some(value) = d.field(handled, "value") {
                        expression(
                            d,
                            value,
                            &format!("rows[{index}].derivations.{}", text(d, name)?),
                            0,
                            visit,
                        )?;
                    }
                }
            }
        }
    }
    if let Some(items) = d
        .field(d.root(), "intermediates")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    {
        for (index, &item) in sequence(d, items)?.iter().enumerate() {
            key(d, item, &format!("intermediates[{index}]"), visit)?;
            if let Some(derivations) = d
                .field(item, "derivations")
                .filter(|&id| !matches!(d.nodes()[id], N::Null))
            {
                for &(name, handled) in mapping(d, derivations)? {
                    let path = format!("intermediates[{index}].derivations.{}", text(d, name)?);
                    let (value, path) = intermediate_derivations::value(d, handled, &path)?;
                    expression(d, value, &path, 0, visit)?;
                }
            }
        }
    }
    Ok(())
}
