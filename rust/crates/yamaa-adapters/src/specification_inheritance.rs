//! Physical inheritance decoding and compatibility representation of engine preparation.
use super::*;
use yamaa_engine::{inheritance_capture as application, inheritance_preparation as lifecycle};

pub use application::CapturePort as InheritancePort;
pub type CapturedParent = application::CapturedParent<DecodedYaml>;
pub type SourceFailure<E> = application::SourceFailure<E, Error>;
#[derive(Debug)]
pub enum InheritanceError<E> {
    Entry(Error),
    Preparation {
        schema: Arc<CapturedSchema>,
        error: Box<lifecycle::Error<SourceFailure<E>>>,
        captured_parents: Vec<CapturedParent>,
    },
}
pub(super) struct Decoder {
    limits: DecodeLimits,
}
impl Decoder {
    pub(super) fn new(limits: DecodeLimits) -> Self {
        Self { limits }
    }
}
impl application::DecodePort for Decoder {
    type Raw = DecodedYaml;
    type Error = Error;
    fn decode(&mut self, identity: &str, bytes: &[u8]) -> Result<DecodedYaml, Error> {
        decode_yaml(bytes, self.limits).map_err(|error| Error::Decode {
            identity: identity.into(),
            error,
        })
    }
    fn document(raw: &DecodedYaml) -> &yamaa_core::schema::Document {
        &raw.document
    }
    fn limit_error(resource: application::CaptureResource) -> Error {
        Error::Limit(match resource {
            application::CaptureResource::CapturedBytes => "captured_bytes",
            application::CaptureResource::IdentityBytes => "identity_bytes",
        })
    }
}
impl CapturedSchema {
    /// Preserve raw YAML captures while the engine prepares the inherited model.
    pub fn prepare_inherited<P: InheritancePort>(
        self: &Arc<Self>,
        source: Source,
        display_path: String,
        port: &mut P,
    ) -> Result<PreparedDocument, InheritanceError<P::Error>> {
        let raw = self
            .decode_entry(&source)
            .map_err(InheritanceError::Entry)?;
        self.prepare_inherited_decoded(source, display_path, raw, port)
    }
    pub(super) fn prepare_inherited_decoded<P: InheritancePort>(
        self: &Arc<Self>,
        source: Source,
        display_path: String,
        raw: DecodedYaml,
        port: &mut P,
    ) -> Result<PreparedDocument, InheritanceError<P::Error>> {
        let prepared = application::prepare(
            &self.structure,
            &source,
            display_path,
            &raw,
            port,
            &mut Decoder {
                limits: self.limits.decode,
            },
            application::Limits {
                captured_bytes: self.limits.captured_bytes,
                identity_bytes: self.limits.identity_bytes,
                normalization: self.limits.normalization,
                ..Default::default()
            },
        );
        match prepared {
            Ok(prepared) => Ok(PreparedDocument {
                schema: Arc::clone(self),
                source,
                raw,
                content: PreparedContent::Inherited(Box::new(prepared.inheritance)),
                parents: prepared.parents,
            }),
            Err(application::Error::Entry(error)) => Err(InheritanceError::Entry(error)),
            Err(application::Error::Preparation {
                error,
                captured_parents,
            }) => Err(InheritanceError::Preparation {
                schema: Arc::clone(self),
                error,
                captured_parents,
            }),
        }
    }
}
