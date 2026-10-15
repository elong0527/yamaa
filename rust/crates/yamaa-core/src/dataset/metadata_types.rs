//! Declaration/header type witnesses over an admitted plan, without any cells.
use super::*;
use crate::{
    aggregate_compiler::AggregateOperand,
    aggregate_parser::Reducer,
    diagnostic::{Diagnostic, ProjectionBudget, ProjectionError},
    value::{ColumnType, ValueType},
};

struct Findings {
    diagnostics: Vec<Diagnostic>,
    maximum: usize,
    bytes: ProjectionBudget,
}
impl Findings {
    fn admit(&mut self, path: &str, expression: &str, source: &str) -> Result<(), ProjectionError> {
        if self.diagnostics.len() >= self.maximum {
            return Err(ProjectionError::Limit);
        }
        self.bytes.charge(1024)?;
        for text in [path, expression, source] {
            self.bytes.text(text)?;
        }
        Ok(())
    }
}
impl DatasetPlan {
    /// Only operand kinds are observed. Zero findings grants no value admission,
    /// function invocation, evaluation, verification or publication authority.
    pub fn metadata_type_findings(
        &self,
        maximum: usize,
    ) -> Result<Vec<Diagnostic>, ProjectionError> {
        let mut findings = Findings {
            diagnostics: Vec::new(),
            maximum,
            bytes: ProjectionBudget::new(crate::diagnostic::MAX_PROJECTION_BYTES)?,
        };
        self.metadata_types(&mut findings)?;
        Ok(findings.diagnostics)
    }
    fn metadata_types(&self, findings: &mut Findings) -> Result<(), ProjectionError> {
        for template in &self.templates {
            let (primary, _) = template
                .mode
                .context(&self.source, &self.secondary, &self.intermediates)
                .map_err(|_| ProjectionError::InvalidContext)?;
            for assignment in &template.assignments {
                self.expression_types(primary, &assignment.expression, &assignment.path, findings)?;
            }
        }
        for assignment in &self.columns {
            self.expression_types(
                &self.source,
                &assignment.expression,
                &assignment.path,
                findings,
            )?;
        }
        for item in &self.intermediates {
            if let Some(derived) = &item.derivations {
                derived.plan.metadata_types(findings)?;
            }
        }
        Ok(())
    }
    fn read_kind(&self, primary: &TableSchema, read: &Read) -> Option<ValueType> {
        let (schema, column) = match read {
            Read::Source(column) => (primary, *column),
            Read::Column(column) => (&self.output, *column),
            Read::Intermediate { index, column } => (
                self.intermediates
                    .get(*index)?
                    .source_schema(&self.secondary)?,
                *column,
            ),
            Read::Joined(read) => (read.schema.as_ref(), read.column),
        };
        schema
            .columns()
            .get(column)
            .map(|column| column.kind.into())
    }
    fn expression_types(
        &self,
        primary: &TableSchema,
        expression: &Expression,
        path: &str,
        findings: &mut Findings,
    ) -> Result<(), ProjectionError> {
        match expression {
            Expression::Flag(flag) => {
                let conditions = flag.condition().type_conditions(|name| {
                    let binding = flag
                        .bindings()
                        .iter()
                        .find(|binding| binding.name == name)?;
                    match &binding.read {
                        FlagRead::Read(read) => self.read_kind(primary, read),
                        FlagRead::Collect { column, .. } => self
                            .source
                            .columns()
                            .get(*column)
                            .map(|column| column.kind.into()),
                    }
                });
                for (condition, route) in conditions {
                    findings.admit(flag.condition().spec_path(), "", "")?;
                    findings.diagnostics.push(flags::type_condition_diagnostic(
                        flag.condition().spec_path(),
                        &condition,
                        &route,
                    ));
                }
            }
            Expression::Aggregate(aggregate) => {
                let compiled = aggregate.expression();
                let schema =
                    aggregate
                        .derived_schema()
                        .unwrap_or_else(|| match aggregate.relation() {
                            AggregateRelation::Output { .. } => &self.output,
                            AggregateRelation::Matched {
                                source: AggregateSource::Secondary(index),
                                ..
                            } => &self.secondary[*index].schema,
                            AggregateRelation::CurrentGroup => primary,
                            _ => &self.source,
                        });
                for reduction in compiled.reductions() {
                    let reducer = match reduction.reducer {
                        Reducer::Sum => NumericReducer::Sum,
                        Reducer::Mean => NumericReducer::Mean,
                        _ => continue,
                    };
                    let AggregateOperand::Value(name) = &reduction.operand else {
                        continue;
                    };
                    let Some(binding) = aggregate
                        .bindings()
                        .iter()
                        .find(|binding| binding.name == *name)
                    else {
                        continue;
                    };
                    let actual = schema.columns()[binding.column].kind;
                    if !matches!(actual, ColumnType::Int | ColumnType::Float) {
                        findings.admit(compiled.spec_path(), compiled.expression(), name)?;
                        findings.diagnostics.push(
                            crate::diagnostic::DatasetFinding::ReductionInputType {
                                path: compiled.spec_path(),
                                expression: compiled.expression(),
                                reducer,
                                source: name,
                                actual: actual.into(),
                            }
                            .diagnostic()
                            .ok_or(ProjectionError::InvalidContext)?,
                        );
                    }
                }
                for derived in aggregate.derivations() {
                    self.expression_types(
                        primary,
                        &derived.assignment.expression,
                        &derived.assignment.path,
                        findings,
                    )?;
                }
            }
            Expression::Case(branches) => {
                for branch in branches {
                    self.expression_types(
                        primary,
                        &branch.assignment.expression,
                        &branch.assignment.path,
                        findings,
                    )?;
                }
            }
            Expression::MappingCut { source, .. }
            | Expression::StrContains { source, .. }
            | Expression::StrExtract { source, .. }
            | Expression::StrCase { source, .. }
            | Expression::StrPad { source, .. }
            | Expression::SourceBinding { source, .. }
            | Expression::Round { source, .. } => {
                self.expression_types(primary, &source.expression, &source.path, findings)?
            }
            Expression::Extreme { operands, .. }
            | Expression::StrConcat { operands, .. }
            | Expression::StrTemplate { operands, .. }
            | Expression::Imputation { operands, .. }
            | Expression::Temporal { operands, .. } => {
                for operand in operands {
                    self.expression_types(primary, &operand.expression, &operand.path, findings)?;
                }
            }
            Expression::Reduce {
                identifier,
                column,
                reducer,
                text,
            } => {
                let source = &primary.columns()[*column];
                if !matches!(source.kind, ColumnType::Int | ColumnType::Float) {
                    let name = identifier.as_deref().unwrap_or(&source.name);
                    findings.admit(path, text, name)?;
                    findings.diagnostics.push(
                        crate::diagnostic::DatasetFinding::ReductionInputType {
                            path,
                            expression: text,
                            reducer: *reducer,
                            source: name,
                            actual: source.kind.into(),
                        }
                        .diagnostic()
                        .ok_or(ProjectionError::InvalidContext)?,
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }
}
