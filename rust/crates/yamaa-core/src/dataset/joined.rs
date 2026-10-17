//! One implicit source operand retains its donor identity and completed key pairs.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinedRead {
    pub source: AggregateSource,
    pub column: usize,
    pub keys: Vec<MatchKey>,
    /// Row-template joins read these raw driver fields; column joins read output keys.
    pub record_keys: Option<Vec<usize>>,
    pub name: String,
    pub schema: std::sync::Arc<TableSchema>,
}
impl JoinedRead {
    pub(super) fn validate(
        &self,
        primary: &TableSchema,
        secondary: &[SecondarySource],
        output: &TableSchema,
        available: &[bool],
        mode: &RowMode,
    ) -> Result<(), PlanError> {
        if self.record_keys.is_some() && matches!(mode.local_mode(), RowMode::Keys) {
            return Err(PlanError::InvalidLookup);
        }
        let donor = match self.source {
            AggregateSource::Primary => &self.schema,
            AggregateSource::Secondary(index) => {
                &secondary.get(index).ok_or(PlanError::InvalidLookup)?.schema
            }
        };
        if self.column >= donor.columns().len() || self.keys.is_empty() {
            return Err(PlanError::InvalidLookup);
        }
        if self
            .record_keys
            .as_ref()
            .is_some_and(|keys| keys.len() != self.keys.len())
        {
            return Err(PlanError::InvalidLookup);
        }
        for (index, key) in self.keys.iter().enumerate() {
            if key.output_column >= output.columns().len() {
                return Err(PlanError::InvalidLookup);
            }
            let field = donor
                .columns()
                .get(key.source_column)
                .ok_or(PlanError::InvalidLookup)?;
            let current = if let Some(keys) = &self.record_keys {
                let column = keys[index];
                if matches!(mode.local_mode(), RowMode::Groups(group) if !group.contains(&column)) {
                    return Err(PlanError::NonGroupSource);
                }
                primary
                    .columns()
                    .get(column)
                    .ok_or(PlanError::InvalidLookup)?
            } else {
                if !available.get(key.output_column).copied().unwrap_or(false) {
                    return Err(PlanError::InvalidLookup);
                }
                output
                    .columns()
                    .get(key.output_column)
                    .ok_or(PlanError::InvalidLookup)?
            };
            if !crate::key_relation::comparable(field.kind.into(), current.kind.into())
                || self.keys[..index]
                    .iter()
                    .any(|prior| prior.source_column == key.source_column)
            {
                return Err(PlanError::InvalidLookup);
            }
        }
        Ok(())
    }
}

pub(super) fn validate_source(
    read: &JoinedRead,
    primary: &TableSchema,
    secondary: &[SecondarySource],
) -> Result<(), PlanError> {
    let actual = match read.source {
        AggregateSource::Primary => primary,
        AggregateSource::Secondary(index) => {
            &secondary.get(index).ok_or(PlanError::InvalidLookup)?.schema
        }
    };
    if read.schema.as_ref() != actual || read.name.is_empty() {
        return Err(PlanError::InvalidSource);
    }
    Ok(())
}
pub(super) fn validate_sources(
    expression: &Expression,
    primary: &TableSchema,
    secondary: &[SecondarySource],
) -> Result<(), PlanError> {
    let read = |read: &JoinedRead| validate_source(read, primary, secondary);
    let binding = |binding: &crate::bound_expression::Binding| match &binding.read {
        Read::Joined(joined) => read(joined),
        _ => Ok(()),
    };
    match expression {
        Expression::Joined(joined) | Expression::JoinedSource { read: joined, .. } => read(joined)?,
        Expression::Compute(numeric) => {
            for value in numeric.bindings() {
                binding(value)?;
            }
        }
        Expression::Function(function) => {
            for argument in function.arguments() {
                if let FunctionInput::Read(Read::Joined(joined)) = &argument.input {
                    read(joined)?;
                }
            }
        }
        Expression::ProjectFunction(function) => {
            for argument in function.arguments() {
                if let FunctionInput::Read(Read::Joined(joined)) = &argument.input {
                    read(joined)?;
                }
            }
        }
        Expression::Flag(flag) => {
            for value in flag.bindings() {
                if let FlagRead::Read(Read::Joined(joined)) = &value.read {
                    read(joined)?;
                }
            }
        }
        Expression::Case(branches) => {
            for branch in branches {
                if let Some(when) = &branch.when {
                    for value in when.bindings() {
                        binding(value)?;
                    }
                }
                validate_sources(&branch.assignment.expression, primary, secondary)?;
            }
        }
        Expression::Extreme { operands, .. }
        | Expression::StrTemplate { operands, .. }
        | Expression::Temporal { operands, .. }
        | Expression::Imputation { operands, .. } => {
            for operand in operands {
                validate_sources(&operand.expression, primary, secondary)?;
            }
        }
        Expression::Round { source, .. }
        | Expression::StrPad { source, .. }
        | Expression::StrCase { source, .. }
        | Expression::StrExtract { source, .. }
        | Expression::StrReplace { source, .. }
        | Expression::MappingCut { source, .. }
        | Expression::SourceBinding { source, .. } => {
            validate_sources(&source.expression, primary, secondary)?
        }
        Expression::FirstAvailable(selection) => {
            for source in selection.sources() {
                if let SelectionRead::Expression(operand) = &source.read {
                    validate_sources(&operand.expression, primary, secondary)?;
                }
            }
        }
        Expression::Aggregate(aggregate) => {
            for derivation in aggregate.derivations() {
                validate_sources(&derivation.assignment.expression, primary, secondary)?;
            }
        }
        _ => (),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{table::Column, value::ColumnType};
    #[test]
    fn raw_join_keys_validate_current_record_coordinates_and_group_membership() {
        let schema = std::sync::Arc::new(
            TableSchema::new(vec![Column {
                name: "K".into(),
                kind: ColumnType::Str,
            }])
            .unwrap(),
        );
        let output = TableSchema::new(vec![Column {
            name: "K".into(),
            kind: ColumnType::Int,
        }])
        .unwrap();
        let mut read = JoinedRead {
            source: AggregateSource::Primary,
            column: 0,
            keys: vec![MatchKey {
                source_column: 0,
                output_column: 0,
            }],
            record_keys: Some(vec![0]),
            name: "RIGHT".into(),
            schema: schema.clone(),
        };
        assert!(read
            .validate(&schema, &[], &output, &[false], &RowMode::Records)
            .is_ok());
        assert_eq!(
            read.validate(&schema, &[], &output, &[false], &RowMode::Groups(vec![])),
            Err(PlanError::NonGroupSource)
        );
        read.record_keys = Some(vec![1]);
        assert_eq!(
            read.validate(&schema, &[], &output, &[false], &RowMode::Records),
            Err(PlanError::InvalidLookup)
        );
        read.record_keys = Some(vec![]);
        assert_eq!(
            read.validate(&schema, &[], &output, &[false], &RowMode::Records),
            Err(PlanError::InvalidLookup)
        );
    }
}
