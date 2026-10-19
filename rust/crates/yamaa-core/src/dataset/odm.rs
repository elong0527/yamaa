//! Fixed ODM item reads bound to an admitted original input and authored row scope.
use super::*;
use crate::{bound_expression::BoundPredicate, odm_source, value::ColumnType};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OdmScope {
    Keys,
    Records,
    Groups(Vec<usize>),
    Templates(std::sync::Arc<[OdmTemplateScope]>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OdmTemplateScope {
    pub row: String,
    pub driver: usize,
    /// None selects the complete record hierarchy; Some retains only the
    /// hierarchy fields present in the enclosing authored group.
    pub groups: Option<Vec<usize>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OdmRead {
    pub dataset: String,
    pub item: String,
    pub item_oid: String,
    pub path: String,
    pub row: Option<String>,
    pub phase: AggregatePhase,
    pub scope: OdmScope,
    /// None borrows the active row driver. A whole-column read retains the
    /// original admitted source independently of the current template port.
    pub source: Option<AggregateSource>,
    /// Study event, form, and item group selectors, respectively.
    pub selectors: [Option<Vec<String>>; 3],
    pub filter: Option<BoundPredicate>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoundOdm(OdmRead);
fn valid_templates(contexts: &[OdmTemplateScope]) -> bool {
    !contexts.is_empty()
        && contexts.iter().all(|context| {
            !context.row.is_empty()
                && context.groups.as_ref().is_none_or(|fields| {
                    fields.iter().enumerate().all(|(index, field)| {
                        odm_source::HIERARCHY.contains(field) && !fields[..index].contains(field)
                    })
                })
        })
}
impl BoundOdm {
    pub fn new(read: OdmRead) -> Result<Self, PlanError> {
        let valid_scope = match &read.scope {
            OdmScope::Templates(contexts) => {
                read.source.is_some()
                    && read.phase == AggregatePhase::Derivation
                    && read.row.is_none()
                    && valid_templates(contexts)
            }
            _ => true,
        };
        if !valid_scope
            || [&read.dataset, &read.item, &read.item_oid, &read.path]
                .iter()
                .any(|text| text.is_empty())
            || read.row.as_ref().is_some_and(String::is_empty)
            || read.item != std::format!("{}.{}", read.dataset, read.item_oid)
        {
            return Err(PlanError::InvalidSource);
        }
        Ok(Self(read))
    }
    pub fn read(&self) -> &OdmRead {
        &self.0
    }
    /// Defer a column-phase read to the scope of each row's template. The
    /// read was bound against one binding's catalog, whose own dataset is the
    /// driver that binding reads through: `port`, the primary input at 0 or
    /// one past a secondary input's position (#2829).
    pub(crate) fn defer(
        &mut self,
        contexts: std::sync::Arc<[OdmTemplateScope]>,
        port: usize,
    ) -> Result<(), PlanError> {
        if self.0.source.is_none()
            || self.0.phase != AggregatePhase::Derivation
            || self.0.row.is_some()
        {
            return Err(PlanError::InvalidSource);
        }
        if !valid_templates(&contexts) {
            return Err(PlanError::InvalidSource);
        }
        if self.0.source == Some(AggregateSource::Primary) {
            self.0.source = Some(match port.checked_sub(1) {
                Some(index) => AggregateSource::Secondary(index),
                None => AggregateSource::Primary,
            });
        }
        self.0.scope = OdmScope::Templates(contexts);
        Ok(())
    }
    pub(super) fn validate(
        &self,
        source: &TableSchema,
        secondary: &[SecondarySource],
        mode: &RowMode,
    ) -> Result<(), PlanError> {
        let source = match self.0.source {
            None | Some(AggregateSource::Primary) => source,
            Some(AggregateSource::Secondary(index)) => {
                &secondary.get(index).ok_or(PlanError::InvalidSource)?.schema
            }
        };
        if source.columns().len() != odm_source::FIELDS.len()
            || source.columns().iter().enumerate().any(|(index, column)| {
                column.name != odm_source::FIELDS[index] || column.kind != ColumnType::Str
            })
        {
            return Err(PlanError::InvalidSource);
        }
        match (&self.0.scope, mode.local_mode()) {
            (OdmScope::Keys, RowMode::Keys) | (OdmScope::Records, RowMode::Records) => (),
            (OdmScope::Templates(_), RowMode::Records | RowMode::Groups(_)) => (),
            (OdmScope::Groups(fields), RowMode::Groups(groups))
                if fields.iter().enumerate().all(|(index, field)| {
                    odm_source::HIERARCHY.contains(field)
                        && groups.contains(field)
                        && !fields[..index].contains(field)
                }) => {}
            _ => return Err(PlanError::InvalidSource),
        }
        if let Some(filter) = &self.0.filter {
            filter
                .validate(source.columns().len(), &[], false)
                .map_err(PlanError::Filter)?;
        }
        Ok(())
    }
}

pub(super) fn validate_template(
    expression: &Expression,
    template: &RowTemplate,
    index: usize,
    count: usize,
) -> Result<(), PlanError> {
    visit(expression, &mut |bound| {
        let source = match bound.read().source {
            None | Some(AggregateSource::Primary) => 0,
            Some(AggregateSource::Secondary(index)) => {
                index.checked_add(1).ok_or(PlanError::InvalidSource)?
            }
        };
        if source != template.mode.source_index() {
            return Err(PlanError::InvalidSource);
        }
        if let OdmScope::Templates(contexts) = &bound.read().scope {
            let context = contexts.get(index).ok_or(PlanError::InvalidSource)?;
            if contexts.len() != count || context.driver != template.mode.source_index() {
                return Err(PlanError::InvalidSource);
            }
            match (&context.groups, template.mode.local_mode()) {
                (None, RowMode::Records) => (),
                (Some(fields), RowMode::Groups(groups))
                    if fields.iter().copied().eq(odm_source::HIERARCHY
                        .into_iter()
                        .filter(|field| groups.contains(field))) => {}
                _ => return Err(PlanError::InvalidSource),
            }
        }
        Ok(())
    })
}

macro_rules! walk {
    ($expression:ident, $visitor:ident, $recurse:ident, $sources:ident, $depth:ident $(, $mutable:tt)?) => {{
        if $depth > crate::limits::Limits::SEMANTIC_DEPTH {
            return Err(PlanError::InvalidColumns);
        }
        match $expression {
            Expression::Odm(bound) => $visitor(bound)?,
            Expression::SourceBinding { source, .. }
            | Expression::MappingCut { source, .. }
            | Expression::StrExtract { source, .. }
            | Expression::StrReplace { source, .. }
            | Expression::StrCase { source, .. }
            | Expression::StrPad { source, .. }
            | Expression::Round { source, .. } => $recurse(&$($mutable)? source.expression, $visitor, $depth + 1)?,
            Expression::Extreme { operands, .. }
            | Expression::StrTemplate { operands, .. }
            | Expression::Imputation { operands, .. }
            | Expression::Temporal { operands, .. } => {
                for operand in operands {
                    $recurse(&$($mutable)? operand.expression, $visitor, $depth + 1)?;
                }
            }
            Expression::Case(branches) => {
                for branch in branches {
                    $recurse(&$($mutable)? branch.assignment.expression, $visitor, $depth + 1)?;
                }
            }
            Expression::FirstAvailable(selection) => {
                for operand in selection.$sources() {
                    if let SelectionRead::Expression(assignment) = &$($mutable)? operand.read {
                        $recurse(&$($mutable)? assignment.expression, $visitor, $depth + 1)?;
                    }
                }
            }
            Expression::Literal(_) | Expression::Flag(_) | Expression::Compute(_)
            | Expression::Function(_) | Expression::ProjectFunction(_) | Expression::Window(_) | Expression::ContextWindow(_)
            | Expression::Source(_) | Expression::SourceLookup { .. } | Expression::Lookup(_)
            | Expression::RowLookup(_) | Expression::Intermediate { .. } | Expression::Collect { .. }
            | Expression::Column(_) | Expression::Reduce { .. } | Expression::Count { .. }
            | Expression::Aggregate(_) | Expression::Joined(_) | Expression::JoinedSource { .. } => (),
        }
        Ok(())
    }};
}
pub(crate) fn visit(
    expression: &Expression,
    visitor: &mut impl FnMut(&BoundOdm) -> Result<(), PlanError>,
) -> Result<(), PlanError> {
    visit_at(expression, visitor, 0)
}
fn visit_at(
    expression: &Expression,
    visitor: &mut impl FnMut(&BoundOdm) -> Result<(), PlanError>,
    depth: usize,
) -> Result<(), PlanError> {
    walk!(expression, visitor, visit_at, sources, depth)
}
pub(crate) fn visit_mut(
    expression: &mut Expression,
    visitor: &mut impl FnMut(&mut BoundOdm) -> Result<(), PlanError>,
) -> Result<(), PlanError> {
    visit_mut_at(expression, visitor, 0)
}
fn visit_mut_at(
    expression: &mut Expression,
    visitor: &mut impl FnMut(&mut BoundOdm) -> Result<(), PlanError>,
    depth: usize,
) -> Result<(), PlanError> {
    walk!(expression, visitor, visit_mut_at, sources_mut, depth, mut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_odm_metadata_keeps_real_template_shape_validation() {
        let path = "x".repeat(16 * 1024 * 1024 + 1);
        let mut bound = BoundOdm::new(OdmRead {
            dataset: "SRC".into(),
            item: "SRC.IT.VAL".into(),
            item_oid: "IT.VAL".into(),
            path: path.clone(),
            row: None,
            phase: AggregatePhase::Derivation,
            scope: OdmScope::Records,
            source: Some(AggregateSource::Primary),
            selectors: [None, None, None],
            filter: None,
        })
        .unwrap();
        assert_eq!(bound.read().path, path);
        let context = OdmTemplateScope {
            row: "row-1".into(),
            driver: 0,
            groups: Some(vec![odm_source::HIERARCHY[0]]),
        };
        bound.defer(vec![context.clone()].into(), 0).unwrap();
        for invalid in [
            vec![],
            vec![OdmTemplateScope {
                row: String::new(),
                ..context.clone()
            }],
            vec![OdmTemplateScope {
                groups: Some(vec![odm_source::HIERARCHY[0]; 2]),
                ..context.clone()
            }],
        ] {
            assert_eq!(
                bound.defer(invalid.into(), 0),
                Err(PlanError::InvalidSource)
            );
            assert_eq!(
                bound.read().scope,
                OdmScope::Templates(vec![context.clone()].into())
            );
        }
    }

    #[test]
    fn nested_scope_admission_refuses_depth_before_visiting_the_leaf() {
        let bound = BoundOdm::new(OdmRead {
            dataset: "SRC".into(),
            item: "SRC.IT.VAL".into(),
            item_oid: "IT.VAL".into(),
            path: "columns.VAL.derivation.odm".into(),
            row: None,
            phase: AggregatePhase::Derivation,
            scope: OdmScope::Records,
            source: Some(AggregateSource::Primary),
            selectors: [None, None, None],
            filter: None,
        })
        .unwrap();
        let mut expression = Expression::Odm(std::boxed::Box::new(bound));
        for _ in 0..129 {
            expression = Expression::Case(std::vec![CaseBranch {
                when: None,
                assignment: std::boxed::Box::new(Assignment {
                    column: 0,
                    expression,
                    path: "nested".into(),
                }),
            }]);
        }
        let mut effects = 0;
        assert_eq!(
            visit(&expression, &mut |_| {
                effects += 1;
                Ok(())
            }),
            Err(PlanError::InvalidColumns)
        );
        assert_eq!(
            visit_mut(&mut expression, &mut |_| {
                effects += 1;
                Ok(())
            }),
            Err(PlanError::InvalidColumns)
        );
        assert_eq!(effects, 0);
    }
}
