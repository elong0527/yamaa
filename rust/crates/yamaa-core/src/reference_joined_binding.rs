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
        let inferred = match &donor.inference {
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
        };
        let mut keys = Vec::new();
        let mut dependencies = Vec::new();
        for &index in inferred {
            let (name, output_column) = &donor.keys[index];
            if available.is_some_and(|available| !available.contains(output_column)) {
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
            dependencies.push(*output_column);
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
            })),
            kind,
            dependencies,
        ))))
    }
}
