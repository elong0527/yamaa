//! The R host transports the same owned JSON facts and shared version checks.
use crate::handle::boundary;
use extendr_api::prelude::*;

fn text(bytes: &Raw) -> std::result::Result<&str, String> {
    std::str::from_utf8(bytes.as_slice()).map_err(|_| "invalid UTF-8 argument".into())
}

#[extendr]
fn resolve_specification_file(specification: Raw, environment: Robj) -> List {
    boundary(|| {
        let specification = text(&specification)?;
        let environment = if environment.is_null() {
            None
        } else {
            Some(
                environment
                    .as_raw()
                    .ok_or("environment must be raw UTF-8 or NULL")?,
            )
        };
        let environment = environment.as_ref().map(text).transpose()?;
        match yamaa_adapters::resolved_artifact::resolve(
            specification,
            environment,
            yamaa_core::project_function::Language::R,
        ) {
            Ok(artifact) => Ok(list!(
                json = artifact.to_json().map_err(|error| error.to_string())?,
                issues = NULL
            )
            .into_robj()),
            Err(issues) => {
                Ok(list!(json = NULL, issues = crate::issue_frame::frame(&issues)?).into_robj())
            }
        }
    })
}

#[extendr]
fn read_resolved_specification(value: Raw) -> List {
    boundary(|| {
        let artifact =
            yamaa_adapters::resolved_artifact::ResolvedArtifact::from_json(text(&value)?)
                .map_err(|error| error.to_string())?;
        Ok(artifact
            .to_json()
            .map_err(|error| error.to_string())?
            .into_robj())
    })
}

extendr_module! { mod resolved_artifact; fn resolve_specification_file; fn read_resolved_specification; }
