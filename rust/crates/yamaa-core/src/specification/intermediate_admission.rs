//! Applicable-key inference for a lookup that declares no key.
use super::*;
use crate::{
    diagnostic::lookup::{self, Owner},
    key_relation::Inference,
    value::ColumnType,
};

/// Reuse the core REQ-0150 service, retaining its output-key order and first mismatch.
#[allow(clippy::too_many_arguments)]
pub(super) fn inferred_keys(
    catalog: &Catalog,
    output: &TableSchema,
    keys: &[usize],
    donor: &TableSchema,
    donor_kind: &impl Fn(usize) -> Option<ColumnType>,
    admits: &impl Fn(usize) -> bool,
    path: &str,
    intermediate: &str,
    dataset: &str,
    findings: &mut Vec<BindFinding>,
) -> Result<Option<Vec<(String, match_values::Declaration)>>, BindError> {
    let names = keys
        .iter()
        .map(|&key| output.columns()[key].name.as_str())
        .collect::<Vec<_>>();
    let fields = donor
        .columns()
        .iter()
        .enumerate()
        .filter(|(index, _)| admits(*index))
        .map(|(index, column)| (column.name.as_str(), donor_kind(index)))
        .collect::<Vec<_>>();
    let inferred =
        crate::key_relation::infer_partial(catalog, &names, &fields).map_err(BindError::Catalog)?;
    match inferred {
        Inference::Keys { keys } => Ok(Some(
            keys.into_iter()
                .map(|key| {
                    let name = String::from(names[key]);
                    (name.clone(), match_values::Declaration::Variable(name))
                })
                .collect(),
        )),
        Inference::NoApplicableKeys => {
            findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                lookup::no_applicable_keys(path, dataset, &names, true),
            )));
            Ok(None)
        }
        Inference::Incompatible {
            key,
            expected,
            actual,
        } => {
            findings.push(BindFinding::Lookup(LookupFinding::from_diagnostic(
                lookup::key_type(
                    path,
                    Owner::Intermediate(intermediate),
                    names[key],
                    expected.into(),
                    actual.into(),
                ),
            )));
            Ok(None)
        }
        Inference::UndeclaredOutput { .. } => Err(BindError::Internal),
    }
}
