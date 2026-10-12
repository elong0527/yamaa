//! Fixed ODM item reads bound to an admitted original input and authored row scope.
use super::*;
use crate::{bound_expression::BoundPredicate, odm_source, value::ColumnType};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OdmScope {
    Keys,
    Records,
    Groups(Vec<usize>),
    Templates(alloc::rc::Rc<[OdmTemplateScope]>),
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
fn read_bytes(read: &OdmRead) -> Option<usize> {
    [&read.dataset, &read.item, &read.item_oid, &read.path]
        .into_iter()
        .chain(read.row.iter())
        .chain(read.selectors.iter().flatten().flatten())
        .try_fold(0usize, |used, text| used.checked_add(text.len()))
}
fn template_bytes(contexts: &[OdmTemplateScope]) -> Option<usize> {
    if contexts.is_empty() {
        return None;
    }
    contexts.iter().try_fold(0usize, |used, context| {
        if context.row.is_empty()
            || context.groups.as_ref().is_some_and(|fields| {
                fields.iter().enumerate().any(|(index, field)| {
                    !odm_source::HIERARCHY.contains(field) || fields[..index].contains(field)
                })
            })
        {
            return None;
        }
        used.checked_add(context.row.capacity())
            .and_then(|used| used.checked_add(core::mem::size_of::<OdmTemplateScope>()))
            .and_then(|used| {
                used.checked_add(match &context.groups {
                    Some(fields) => fields
                        .capacity()
                        .checked_mul(core::mem::size_of::<usize>())?,
                    None => 0,
                })
            })
    })
}
impl BoundOdm {
    pub fn new(read: OdmRead) -> Result<Self, PlanError> {
        let metadata = read_bytes(&read).and_then(|bytes| match &read.scope {
            OdmScope::Templates(contexts)
                if read.source.is_some()
                    && read.phase == AggregatePhase::Derivation
                    && read.row.is_none() =>
            {
                template_bytes(contexts).and_then(|extra| bytes.checked_add(extra))
            }
            OdmScope::Templates(_) => None,
            _ => Some(bytes),
        });
        if metadata.is_none_or(|bytes| bytes > 16 * 1024 * 1024)
            || [&read.dataset, &read.item, &read.item_oid, &read.path]
                .iter()
                .any(|text| text.is_empty())
            || read.row.as_ref().is_some_and(String::is_empty)
            || read.item != alloc::format!("{}.{}", read.dataset, read.item_oid)
        {
            return Err(PlanError::InvalidSource);
        }
        Ok(Self(read))
    }
    pub fn read(&self) -> &OdmRead {
        &self.0
    }
    pub(crate) fn defer(
        &mut self,
        contexts: alloc::rc::Rc<[OdmTemplateScope]>,
    ) -> Result<(), PlanError> {
        if self.0.source.is_none()
            || self.0.phase != AggregatePhase::Derivation
            || self.0.row.is_some()
        {
            return Err(PlanError::InvalidSource);
        }
        let bytes = read_bytes(&self.0)
            .and_then(|used| template_bytes(&contexts).and_then(|extra| used.checked_add(extra)));
        if bytes.is_none_or(|bytes| bytes > 16 * 1024 * 1024) {
            return Err(PlanError::InvalidSource);
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
    ($expression:ident, $visitor:ident, $recurse:ident, $sources:ident, $depth:ident, $remaining:ident $(, $mutable:tt)?) => {{
        if $depth > EXPRESSION_DEPTH_LIMIT || *$remaining == 0 {
            return Err(PlanError::InvalidColumns);
        }
        *$remaining -= 1;
        match $expression {
            Expression::Odm(bound) => $visitor(bound)?,
            Expression::SourceBinding { source, .. }
            | Expression::MappingCut { source, .. }
            | Expression::StrContains { source, .. }
            | Expression::StrExtract { source, .. }
            | Expression::StrCase { source, .. }
            | Expression::StrPad { source, .. }
            | Expression::Round { source, .. } => $recurse(&$($mutable)? source.expression, $visitor, $depth + 1, $remaining)?,
            Expression::Extreme { operands, .. }
            | Expression::StrConcat { operands, .. }
            | Expression::StrTemplate { operands, .. }
            | Expression::Imputation { operands, .. }
            | Expression::Temporal { operands, .. } => {
                for operand in operands {
                    $recurse(&$($mutable)? operand.expression, $visitor, $depth + 1, $remaining)?;
                }
            }
            Expression::Case(branches) => {
                for branch in branches {
                    $recurse(&$($mutable)? branch.assignment.expression, $visitor, $depth + 1, $remaining)?;
                }
            }
            Expression::FirstAvailable(selection) => {
                for operand in selection.$sources() {
                    if let SelectionRead::Expression(assignment) = &$($mutable)? operand.read {
                        $recurse(&$($mutable)? assignment.expression, $visitor, $depth + 1, $remaining)?;
                    }
                }
            }
            Expression::Literal(_) | Expression::Flag(_) | Expression::Compute(_)
            | Expression::Function(_) | Expression::ProjectFunction(_) | Expression::Window(_)
            | Expression::Source(_) | Expression::SourceLookup { .. } | Expression::Lookup(_)
            | Expression::RowLookup(_) | Expression::Intermediate { .. } | Expression::Collect { .. }
            | Expression::Column(_) | Expression::Reduce { .. } | Expression::Count { .. }
            | Expression::Aggregate(_) => (),
        }
        Ok(())
    }};
}
pub(crate) fn visit(
    expression: &Expression,
    visitor: &mut impl FnMut(&BoundOdm) -> Result<(), PlanError>,
) -> Result<(), PlanError> {
    let mut remaining = EXPRESSION_NODE_LIMIT;
    visit_at(expression, visitor, 0, &mut remaining)
}
fn visit_at(
    expression: &Expression,
    visitor: &mut impl FnMut(&BoundOdm) -> Result<(), PlanError>,
    depth: usize,
    remaining: &mut usize,
) -> Result<(), PlanError> {
    walk!(expression, visitor, visit_at, sources, depth, remaining)
}
pub(crate) fn visit_mut(
    expression: &mut Expression,
    visitor: &mut impl FnMut(&mut BoundOdm) -> Result<(), PlanError>,
) -> Result<(), PlanError> {
    let mut remaining = EXPRESSION_NODE_LIMIT;
    visit_mut_at(expression, visitor, 0, &mut remaining)
}
fn visit_mut_at(
    expression: &mut Expression,
    visitor: &mut impl FnMut(&mut BoundOdm) -> Result<(), PlanError>,
    depth: usize,
    remaining: &mut usize,
) -> Result<(), PlanError> {
    walk!(
        expression,
        visitor,
        visit_mut_at,
        sources_mut,
        depth,
        remaining,
        mut
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut expression = Expression::Odm(alloc::boxed::Box::new(bound));
        for _ in 0..66 {
            expression = Expression::Case(alloc::vec![CaseBranch {
                when: None,
                assignment: alloc::boxed::Box::new(Assignment {
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
