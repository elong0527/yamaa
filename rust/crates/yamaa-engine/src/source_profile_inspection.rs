//! Refine invalid source profiles with metadata-only physical resource findings.
//! Unknown regular or missing paths retain their static profile diagnostic. No
//! byte capture or decoder is available at this boundary.
use alloc::string::String;
use yamaa_core::{
    resource::ResourceFailure,
    specification::{PreflightFinding, PrepareError},
};

pub fn refine(
    error: PrepareError,
    mut inspect: impl FnMut(&str, &str) -> Option<(ResourceFailure, String)>,
) -> PrepareError {
    let PrepareError::Invalid(mut findings) = error else {
        return error;
    };
    for finding in &mut findings {
        let PreflightFinding::UnknownSourceProfile { dataset, path } = finding else {
            continue;
        };
        if let Some((
            failure @ (ResourceFailure::NotRegularFile | ResourceFailure::Symlink),
            written,
        )) = inspect(dataset, path)
        {
            *finding = PreflightFinding::Scalar(failure.diagnostic(dataset, &written));
        }
    }
    PrepareError::Invalid(findings)
}
