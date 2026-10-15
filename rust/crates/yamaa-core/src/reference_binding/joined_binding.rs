//! Callback arguments borrow the same admitted implicit-join metadata as scalar sources.
use super::*;
use crate::{
    bound_expression::Read,
    dataset::{AggregateSource, JoinedRead, MatchKey},
};
use alloc::{boxed::Box, vec::Vec};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JoinedFinding {
    UnknownField,
    CurrentKey {
        identifier: String,
        finding: crate::reference_scope::Finding,
    },
    NoApplicableKeys {
        dataset: String,
        keys: Vec<String>,
    },
    KeyType {
        source: String,
        expected: ColumnType,
        actual: ColumnType,
    },
    UnavailableKey {
        column: usize,
    },
}
type JoinedReadValue = (Read, ColumnType, Vec<usize>);
type JoinedResolution = Result<Option<Result<JoinedReadValue, JoinedFinding>>, Error>;
impl Catalog {
    pub(crate) fn joined_read(&self, name: &str, available: Option<&[usize]>) -> JoinedResolution {
        limit("reference_bytes", name.len(), self.limits.reference_bytes)?;
        let Some((owner, field)) = name.split_once('.') else {
            return Ok(None);
        };
        let Some(donor) = self.joined.get(owner) else {
            return Ok(None);
        };
        let Some(&(column, kind)) = donor.fields.get(field) else {
            return Ok(Some(Err(JoinedFinding::UnknownField)));
        };
        let mut record_keys = None;
        let row_pairs;
        // Raw record keys are the row scope's plan; a donor key the driver does
        // not store raw but the row itself constructs still matches, through
        // the constructed output keys, so the whole read uses that plan.
        let mut raw_pairs = None;
        if let Some(scope) = &self.row_joins {
            use crate::reference_scope::{self, Phase, Reach, Scope};
            let groups = scope
                .groups
                .as_ref()
                .map(|names| names.iter().map(String::as_str).collect::<Vec<_>>());
            let mut pairs = Vec::new();
            let mut records = Vec::new();
            let mut constructed = false;
            for (index, (name, output_column)) in donor.keys.iter().enumerate() {
                let Some(&(_, actual)) = donor.fields.get(name) else {
                    continue;
                };
                let identifier = alloc::format!("{}.{}", scope.driver, name);
                let findings = reference_scope::validate(
                    self,
                    &identifier,
                    None,
                    Scope {
                        drivers: &[&scope.driver],
                        current_driver: false,
                        reach: Reach::Scalar,
                        joined: false,
                        phase: Phase::Row {
                            group_by: groups.as_deref(),
                        },
                    },
                    Default::default(),
                )?;
                if let Some(&finding) = findings.first() {
                    if finding == reference_scope::Finding::UnknownField
                        && scope.row_keys.contains(output_column)
                    {
                        constructed = true;
                        break;
                    }
                    return Ok(Some(Err(JoinedFinding::CurrentKey {
                        identifier,
                        finding,
                    })));
                }
                let Some(Binding::Dataset {
                    field, column_type, ..
                }) = self.bind(&identifier)?
                else {
                    return Err(Error::InvalidAvailableOutput);
                };
                if !crate::key_relation::comparable(column_type.into(), actual.into()) {
                    return Ok(Some(Err(JoinedFinding::KeyType {
                        source: name.clone(),
                        expected: column_type,
                        actual,
                    })));
                }
                pairs.push(index);
                records.push(field);
            }
            if !constructed {
                if pairs.is_empty() {
                    return Ok(Some(Err(JoinedFinding::NoApplicableKeys {
                        dataset: owner.into(),
                        keys: donor.keys.iter().map(|(name, _)| name.clone()).collect(),
                    })));
                }
                record_keys = Some(records);
                raw_pairs = Some(pairs);
            }
        }
        let inferred = if let Some(pairs) = raw_pairs {
            row_pairs = pairs;
            &row_pairs
        } else {
            match &donor.inference {
                crate::key_relation::Inference::Keys { keys } => keys,
                crate::key_relation::Inference::NoApplicableKeys => {
                    return Ok(Some(Err(JoinedFinding::NoApplicableKeys {
                        dataset: owner.into(),
                        keys: donor.keys.iter().map(|(name, _)| name.clone()).collect(),
                    })))
                }
                crate::key_relation::Inference::Incompatible {
                    key,
                    expected,
                    actual,
                } => {
                    return Ok(Some(Err(JoinedFinding::KeyType {
                        source: donor.keys[*key].0.clone(),
                        expected: *expected,
                        actual: *actual,
                    })))
                }
                crate::key_relation::Inference::UndeclaredOutput { .. } => {
                    return Err(Error::InvalidAvailableOutput)
                }
            }
        };
        let mut keys = Vec::new();
        let mut dependencies = Vec::new();
        for &index in inferred {
            let (name, output_column) = &donor.keys[index];
            if record_keys.is_none()
                && available.is_some_and(|available| !available.contains(output_column))
            {
                return Ok(Some(Err(JoinedFinding::UnavailableKey {
                    column: *output_column,
                })));
            }
            let source_column = donor
                .fields
                .get(name)
                .ok_or(Error::InvalidAvailableOutput)?
                .0;
            keys.push(MatchKey {
                source_column,
                output_column: *output_column,
            });
            if record_keys.is_none() {
                dependencies.push(*output_column);
            }
        }
        let source = if donor.source == 0 {
            AggregateSource::Primary
        } else {
            AggregateSource::Secondary(donor.source - 1)
        };
        Ok(Some(Ok((
            Read::Joined(Box::new(JoinedRead {
                source,
                column,
                keys,
                name: owner.into(),
                schema: donor.schema.clone(),
                record_keys,
            })),
            kind,
            dependencies,
        ))))
    }
}
