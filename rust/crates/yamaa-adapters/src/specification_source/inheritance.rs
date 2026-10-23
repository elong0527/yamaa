//! What a specification keeps of the parents its preparation read, and why it fails to prepare.
use super::*;
use yamaa_core::inheritance_capture as application;

pub type CapturedParent = application::CapturedParent<DecodedYaml>;
pub type SourceFailure<E> = application::SourceFailure<E, Error>;
#[derive(Debug)]
pub enum InheritanceError<E> {
    Entry(Error),
    Dictionary {
        document: Box<PreparedDocument>,
        error: Box<yamaa_core::dictionary_preparation::Error<E, Error, DecodedYaml>>,
    },
    /// A specification did not prepare (#3030 D76). `schema` is the 1.0
    /// specification closure, and `entry` the identity of the entry file.
    Versioned {
        schema: Arc<CapturedSchema>,
        entry: String,
        error: Box<crate::document::SpecificationError<E>>,
    },
}
