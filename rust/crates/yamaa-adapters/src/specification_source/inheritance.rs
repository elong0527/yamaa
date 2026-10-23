//! Physical inheritance decoding for retained engine preparation.
use super::*;
use yamaa_core::{
    inheritance_capture as application, inheritance_preparation as lifecycle, resolver::Resolver,
};

pub type CapturedParent = application::CapturedParent<DecodedYaml>;
pub type SourceFailure<E> = application::SourceFailure<E, Error>;
#[derive(Debug)]
pub enum InheritanceError<E> {
    Entry(Error),
    Dictionary {
        document: Box<PreparedDocument>,
        error: Box<yamaa_core::dictionary_preparation::Error<E, Error, DecodedYaml>>,
    },
    Preparation {
        schema: Arc<CapturedSchema>,
        error: Box<lifecycle::Error<SourceFailure<E>>>,
        captured_parents: Vec<CapturedParent>,
    },
    /// A specification that declares the 1.x header did not prepare (#3030
    /// D76). `schema` is the 1.0 specification closure, and `entry` the
    /// identity of the entry file.
    Versioned {
        schema: Arc<CapturedSchema>,
        entry: String,
        error: Box<crate::document::SpecificationError<E>>,
    },
}
pub(super) struct Decoder;
impl application::InheritanceDecodePort for Decoder {
    type Raw = DecodedYaml;
    type Error = Error;
    fn decode(&mut self, identity: &str, bytes: &[u8]) -> Result<DecodedYaml, Error> {
        decode_yaml(bytes).map_err(|error| Error::Decode {
            identity: identity.into(),
            error,
        })
    }
    fn document(raw: &DecodedYaml) -> &yamaa_core::schema::Document {
        &raw.document
    }
}
impl CapturedSchema {
    /// Preserve raw YAML captures while the engine prepares the inherited model.
    pub fn prepare_inherited<P: Resolver>(
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
    pub(super) fn prepare_inherited_decoded<P: Resolver>(
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
            &mut Decoder,
        );
        match prepared {
            Ok(prepared) => Ok(PreparedDocument {
                schema: Arc::clone(self),
                source,
                raw,
                content: PreparedContent::Inherited(Box::new(prepared.inheritance)),
                parents: prepared.parents,
                dictionaries: None,
                resource_aliases: Default::default(),
                legacy_tree: Default::default(),
            }
            .prepare_dictionaries(port)?),
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
