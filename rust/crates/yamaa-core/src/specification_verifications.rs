//! Verification admission and deferred declaration diagnostics. The valid prefix
//! executes only after derivation and output keys; unsupported vocabulary is
//! scanned across every declaration before any source request.
use super::*;
use crate::dataset::{Check, Verification};

#[derive(Clone, Debug)]
struct DeclarationFinding {
    pub path: String,
    pub condition: &'static str,
    pub requirement: &'static str,
    pub reason: String,
}
#[derive(Debug, Default)]
pub(super) struct Verifications {
    pub checks: Vec<Verification>,
    pub identities: Vec<Option<String>>,
    deferred: Option<DeclarationFinding>,
}
fn operation(d: &Document, id: usize) -> Result<(&str, usize), PrepareError> {
    let &[(name, payload)] = mapping(d, id)? else {
        return Err(PrepareError::Internal);
    };
    Ok((text(d, name)?, payload))
}
impl Verifications {
    pub(super) fn prepare(d: &Document, output: &TableSchema) -> Result<Self, PrepareError> {
        let mut result = Self::default();
        let Some(id) = d
            .field(d.root(), "verifications")
            .filter(|&id| !matches!(d.nodes()[id], N::Null))
        else {
            return Ok(result);
        };
        let entries = sequence(d, id)?;
        if entries.len() > 16 {
            return Err(PrepareError::Limit("verifications"));
        }
        let mut extra = Vec::new();
        for (index, &id) in entries.iter().enumerate() {
            let (op, payload) = operation(d, id)?;
            let path = format!("verifications[{index}].{op}");
            let allowed: &[&str] = match op {
                "unique" => &["columns", "id", "severity"],
                "row_count" => &["min", "max", "id", "severity"],
                _ => {
                    reject(&mut extra, op, path);
                    continue;
                }
            };
            if op == "unique" && matches!(d.nodes()[payload], N::Sequence(_)) {
                continue;
            }
            for &(name, _) in mapping(d, payload)? {
                let name = text(d, name)?;
                if !allowed.contains(&name) {
                    reject(&mut extra, name, format!("{path}.{name}"));
                }
            }
            if let Some(id) = d.field(payload, "severity") {
                if text(d, id)? != "error" {
                    reject(&mut extra, "verification_severity", path.clone());
                }
            }
            // The current dataset engine's count bounds are signed 64-bit. Do
            // not lose arbitrary-width authored bounds through a narrowing cast.
            for name in if op == "row_count" {
                &["min", "max"][..]
            } else {
                &[]
            } {
                if let Some(id) = d.field(payload, name) {
                    if let N::Integer(value) = &d.nodes()[id] {
                        if value.parse::<i64>().is_err() {
                            reject(&mut extra, "verification_bound", format!("{path}.{name}"));
                        }
                    }
                }
            }
        }
        if !extra.is_empty() {
            return Err(PrepareError::Unsupported(extra));
        }
        let mut quote_budget = crate::schema::ValidationBudget::new(Default::default());
        let mut quote = |value: &str| {
            crate::schema::quoted_diagnostic_text(value, &mut quote_budget)
                .map_err(|_| PrepareError::Limit("verification_diagnostic"))
        };
        let mut ids = BTreeMap::<String, String>::new();
        for (index, &id) in entries.iter().enumerate() {
            let (op, payload) = operation(d, id)?;
            let path = format!("verifications[{index}].{op}");
            let invalid = |path: String, condition, requirement, reason: String| {
                Some(DeclarationFinding {
                    path,
                    condition,
                    requirement,
                    reason,
                })
            };
            let identity = if let Some(id) = d.field(payload, "id") {
                match &d.nodes()[id] {
                    N::Text(value) if !value.is_empty() => {
                        if let Some(previous) = ids.get(value) {
                            result.deferred = invalid(
                                path,
                                "duplicate_identifier",
                                "REQ-0398",
                                format!("verification id {} repeats {previous}", quote(value)?),
                            );
                            break;
                        }
                        ids.insert(value.clone(), path.clone());
                        Some(value.clone())
                    }
                    _ => {
                        result.deferred = invalid(
                            path,
                            "invalid_declaration",
                            "REQ-0374",
                            "a verification id is text".into(),
                        );
                        break;
                    }
                }
            } else {
                None
            };
            let check = if op == "unique" {
                let values = if matches!(d.nodes()[payload], N::Sequence(_)) {
                    sequence(d, payload)?
                } else {
                    sequence(d, field(d, payload, "columns")?)?
                };
                if values.is_empty() {
                    result.deferred = invalid(
                        path,
                        "invalid_declaration",
                        "REQ-0397",
                        "columns names at least one column".into(),
                    );
                    break;
                }
                let mut columns = Vec::new();
                for &id in values {
                    let name = text(d, id)?;
                    if let Some(column) = output.columns().iter().position(|c| c.name == name) {
                        columns.push(column);
                    } else {
                        result.deferred = invalid(
                            format!("{path}.columns"),
                            "unknown_field",
                            "REQ-0405",
                            format!("unknown column {}", quote(name)?),
                        );
                        break;
                    }
                }
                if result.deferred.is_some() {
                    break;
                }
                Check::Unique(columns)
            } else {
                let bound = |name| {
                    d.field(payload, name)
                        .filter(|&id| !matches!(d.nodes()[id], N::Null))
                        .map(|id| {
                            let N::Integer(value) = &d.nodes()[id] else {
                                return Err(PrepareError::Internal);
                            };
                            value.parse::<i64>().map_err(|_| PrepareError::Internal)
                        })
                        .transpose()
                };
                let min = bound("min")?;
                let max = bound("max")?;
                let reason = if min.is_none() && max.is_none() {
                    Some("row_count requires one bound")
                } else if matches!((min,max),(Some(a),Some(b)) if a>b) {
                    Some("row_count min exceeds max")
                } else {
                    None
                };
                if let Some(reason) = reason {
                    result.deferred =
                        invalid(path, "invalid_declaration", "REQ-0399", reason.into());
                    break;
                }
                Check::RowCount { min, max }
            };
            result.checks.push(Verification { path, check });
            result.identities.push(identity);
        }
        if let Some(finding) = &result.deferred {
            result.checks.push(Verification {
                path: finding.path.clone(),
                check: Check::InvalidDeclaration {
                    condition: finding.condition,
                    requirement: finding.requirement,
                    reason: finding.reason.clone(),
                },
            });
            result.identities.push(None);
        }
        Ok(result)
    }
}
