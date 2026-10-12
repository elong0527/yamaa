//! Fixed ODM item reads bound to an admitted original input and authored row scope.
use super::*;
use crate::{bound_expression::BoundPredicate, odm_source, value::ColumnType};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OdmScope {
    Keys,
    Records,
    Groups(Vec<usize>),
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
    /// Study event, form, and item group selectors, respectively.
    pub selectors: [Option<Vec<String>>; 3],
    pub filter: Option<BoundPredicate>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoundOdm(OdmRead);
impl BoundOdm {
    pub fn new(read: OdmRead) -> Result<Self, PlanError> {
        let metadata = [&read.dataset, &read.item, &read.item_oid, &read.path]
            .into_iter()
            .chain(read.row.iter())
            .chain(read.selectors.iter().flatten().flatten())
            .try_fold(0usize, |used, text| used.checked_add(text.len()));
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
    pub(super) fn validate(&self, source: &TableSchema, mode: &RowMode) -> Result<(), PlanError> {
        if source.columns().len() != odm_source::FIELDS.len()
            || source.columns().iter().enumerate().any(|(index, column)| {
                column.name != odm_source::FIELDS[index] || column.kind != ColumnType::Str
            })
        {
            return Err(PlanError::InvalidSource);
        }
        match (&self.0.scope, mode.local_mode()) {
            (OdmScope::Keys, RowMode::Keys) | (OdmScope::Records, RowMode::Records) => (),
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
