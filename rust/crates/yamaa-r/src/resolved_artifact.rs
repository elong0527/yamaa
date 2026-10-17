//! The R host transports the same owned JSON facts and shared version checks.
use crate::handle::boundary;
use extendr_api::prelude::*;
use yamaa::{Engine, EngineOptions, HostLanguage, Request};

fn text(bytes: &Raw) -> std::result::Result<&str, String> {
    std::str::from_utf8(bytes.as_slice()).map_err(|_| "invalid UTF-8 argument".into())
}

#[extendr]
fn resolve_specification_file(specification: Raw, environment: Robj) -> List {
    boundary(|| {
        let specification = text(&specification)?.to_owned();
        let environment = if environment.is_null() {
            None
        } else {
            Some(
                environment
                    .as_raw()
                    .ok_or("environment must be raw UTF-8 or NULL")?,
            )
        };
        let environment = environment
            .as_ref()
            .map(text)
            .transpose()?
            .map(str::to_owned);
        // The R host has no runtime version at this entry point; resolution
        // never renders a report identity, so the version stays empty.
        let engine = Engine::new(EngineOptions::new(
            HostLanguage::R,
            "r".into(),
            String::new(),
        ));
        let mut request = Request::new(&specification);
        if let Some(environment) = environment.as_deref() {
            request = request.with_environment(environment);
        }
        match engine.resolve_specification(&request) {
            Ok(artifact) => Ok(list!(
                json = artifact.to_json().map_err(|error| error.to_string())?,
                issues = NULL
            )
            .into_robj()),
            Err(yamaa::Error::Rejected(report)) => Ok(list!(
                json = NULL,
                issues = crate::issue_frame::frame(&report.diagnostics)?
            )
            .into_robj()),
            Err(error) => Err(error.to_string()),
        }
    })
}

#[extendr]
fn read_resolved_specification(value: Raw) -> List {
    boundary(|| {
        let artifact = yamaa::ResolvedSpecification::from_json(text(&value)?)
            .map_err(|error| error.to_string())?;
        Ok(artifact
            .to_json()
            .map_err(|error| error.to_string())?
            .into_robj())
    })
}

include!("generated/resolved_artifact.rs");
