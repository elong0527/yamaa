//! A file-system resolver for tests that prepare written documents.
use std::{fs, io, path::Path, sync::Arc};
use yamaa_core::resolver::{Captured, Failure, Located, Name, Resolver};
use yamaa_core::resource::ResourceFailure;

/// Metadata files resolve beside their declaring file, named by the standard
/// library's canonical path; input paths stay as written. A located file is
/// read at its canonical path. A file that cannot be located is missing; a
/// file that cannot be read keeps its error.
pub struct Files;
impl Resolver for Files {
    type Error = io::Error;
    fn inspect(&mut self, name: Name<'_>) -> Result<Located, io::Error> {
        let Name::Written {
            declaring, written, ..
        } = name
        else {
            panic!("the test locates metadata files only, not {name:?}")
        };
        let candidate = Path::new(declaring).parent().unwrap().join(written);
        let identity = candidate.canonicalize()?;
        Ok(Located {
            identity: identity.to_str().unwrap().into(),
            display_path: candidate.to_str().unwrap().into(),
        })
    }
    fn capture(&mut self, name: Name<'_>) -> Result<Captured, io::Error> {
        let Name::Located { source, .. } = name else {
            panic!("the test captures located files only, not {name:?}")
        };
        let bytes = fs::read(&source.identity)?;
        Ok(Captured::resource(Arc::from(bytes), true))
    }
    fn failure(&self, name: Name<'_>, _: &io::Error) -> Option<Failure> {
        matches!(name, Name::Written { .. }).then_some(Failure::Resource(ResourceFailure::Missing))
    }
    fn rebase(&mut self, _: &Located, _: &Located, written: &str) -> Result<String, io::Error> {
        Ok(written.into())
    }
}
