//! Lower only governed column codelist bindings; retain all other submission gates.
use super::*;
use crate::{dataset::Check, project_codelist_binding as ct, project_terminology::Catalogue};

pub(super) fn submission_features(
    d: &Document,
    column: usize,
    prefix: &str,
    catalogue: bool,
    extra: &mut Vec<UnsupportedFeature>,
) -> Result<(), PrepareError> {
    let Some(submission) = d
        .field(column, "submission")
        .filter(|&id| !matches!(d.nodes()[id], N::Null))
    else {
        return Ok(());
    };
    if !catalogue {
        optional_features(d, column, &["submission"], prefix, extra);
        return Ok(());
    }
    for &(name, value) in mapping(d, submission)? {
        let name = text(d, name)?;
        if name != "codelist" && !matches!(d.nodes()[value], N::Null) {
            reject(extra, "submission", format!("{prefix}.submission.{name}"));
        }
    }
    Ok(())
}
pub(super) fn apply(
    d: &Document,
    columns: &[usize],
    output: &TableSchema,
    catalogue: &Catalogue,
    limits: crate::project_limits::Limits,
    groups: &mut [verifications::Verifications],
) -> Result<(), PrepareError> {
    let declarations = columns
        .iter()
        .enumerate()
        .filter_map(|(column, &id)| {
            let identifier = d
                .field(id, "submission")
                .and_then(|id| d.field(id, "codelist"))
                .filter(|&id| !matches!(d.nodes()[id], N::Null))?;
            Some((column, identifier))
        })
        .collect::<Vec<_>>();
    let allowed = declarations
        .iter()
        .map(|&(column, _)| {
            groups[column]
                .checks
                .iter()
                .filter_map(|check| {
                    if let Check::AllowedValues(values) = &check.check {
                        Some(ct::AllowedValues {
                            path: &check.path,
                            values,
                        })
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let bindings = declarations
        .iter()
        .enumerate()
        .map(|(index, &(column, identifier))| {
            Ok(ct::Binding {
                column,
                name: &output.columns()[column].name,
                kind: output.columns()[column].kind,
                path: ct::Path::Column(&output.columns()[column].name),
                codelist: text(d, identifier)?,
                allowed_values: &allowed[index],
            })
        })
        .collect::<Result<Vec<_>, PrepareError>>()?;
    let checks = ct::bind(catalogue, &bindings, limits).map_err(|error| match error {
        ct::Error::Limit(_) => PrepareError::Limit("terminology_bindings"),
        ct::Error::Findings(findings) => PrepareError::Invalid(
            findings
                .into_iter()
                .map(|finding| PreflightFinding::ProjectCodelist(alloc::boxed::Box::new(finding)))
                .collect(),
        ),
    })?;
    for check in checks {
        groups[check.column].append_codelist(check.verification);
    }
    Ok(())
}
