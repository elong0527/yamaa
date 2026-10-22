//! A file-system inheritance port for tests that prepare written documents.
use std::{fs, path::Path};
use yamaa_adapters::specification_source::InheritancePort;
use yamaa_core::inheritance::{Source as GraphSource, SourceError};

/// Parents resolve beside their declaring file, named by the standard
/// library's canonical path; input paths stay as written.
pub struct Files;
impl InheritancePort for Files {
    type Error = std::io::Error;
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<GraphSource, SourceError<Self::Error>> {
        let candidate = Path::new(declaring).parent().unwrap().join(written);
        let identity = candidate
            .canonicalize()
            .map_err(|_| SourceError::Unavailable)?;
        Ok(GraphSource {
            identity: identity.to_str().unwrap().into(),
            display_path: candidate.to_str().unwrap().into(),
        })
    }
    fn capture(&mut self, source: &GraphSource) -> Result<Vec<u8>, SourceError<Self::Error>> {
        fs::read(&source.identity).map_err(SourceError::Raised)
    }
    fn rebase(
        &mut self,
        _: &GraphSource,
        _: &GraphSource,
        written: &str,
    ) -> Result<String, Self::Error> {
        Ok(written.into())
    }
}
