//! One decoded entry selects the standalone or inherited preparation lifecycle.
//! Codecs retain physical source/location authority; this application owns no IO.
use crate::{inheritance_capture, project_preparation::Source, standalone_preparation};
use alloc::string::String;
use yamaa_core::schema::{Document, NormalizationBudget, SchemaStructure};

/// Entry source admission is distinct from parent capture admission. The borrowed
/// document must be the exact arena held by `Raw`; equal text is not authority.
pub trait EntryCodec {
    type Raw;
    type Locations;
    type Error;
    fn decode(&mut self, source: &Source) -> Result<Self::Raw, Self::Error>;
    fn document(raw: &Self::Raw) -> &Document;
    fn into_parts(raw: Self::Raw) -> (Document, Self::Locations);
}
#[derive(Debug)]
pub enum Content<R, L> {
    Standalone {
        prepared: standalone_preparation::PreparedStandalone,
        locations: L,
    },
    Inherited {
        raw: R,
        prepared: inheritance_capture::Prepared<R>,
    },
}
#[derive(Debug)]
pub struct Prepared<R, L> {
    source: Source,
    content: Content<R, L>,
}
impl<R, L> Prepared<R, L> {
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn content(&self) -> &Content<R, L> {
        &self.content
    }
    pub fn into_parts(self) -> (Source, Content<R, L>) {
        (self.source, self.content)
    }
}
#[derive(Debug)]
pub enum Error<E, X, R> {
    Entry(X),
    Standalone {
        source: Source,
        error: standalone_preparation::Error,
    },
    Inherited(inheritance_capture::Error<E, X, R>),
}
pub type PreparationResult<E, X, R, L> = Result<Prepared<R, L>, Error<E, X, R>>;

/// Decode once, then test raw parent presence without a preceding normalization
/// or version/default gate. Both routes retain their own existing checkpoint and
/// cumulative budget semantics. Null/scalar/empty parents still select inheritance.
pub fn prepare<E, D, P>(
    schema: &SchemaStructure,
    source: Source,
    display_path: String,
    entry: &mut E,
    parents: &mut D,
    capture: &mut P,
    limits: inheritance_capture::Limits,
) -> PreparationResult<P::Error, E::Error, E::Raw, E::Locations>
where
    E: EntryCodec,
    D: inheritance_capture::DecodePort<Raw = E::Raw, Error = E::Error>,
    P: inheritance_capture::CapturePort,
{
    let raw = entry.decode(&source).map_err(Error::Entry)?;
    let content = if E::document(&raw)
        .field(E::document(&raw).root(), "parents")
        .is_some()
    {
        let prepared = inheritance_capture::prepare(
            schema,
            &source,
            display_path,
            &raw,
            capture,
            parents,
            limits,
        )
        .map_err(Error::Inherited)?;
        Content::Inherited { raw, prepared }
    } else {
        let (raw, locations) = E::into_parts(raw);
        let prepared = match standalone_preparation::prepare(
            schema,
            raw,
            &mut NormalizationBudget::new(limits.normalization),
        ) {
            Ok(prepared) => prepared,
            Err(error) => return Err(Error::Standalone { source, error }),
        };
        return Ok(Prepared {
            source,
            content: Content::Standalone {
                prepared,
                locations,
            },
        });
    };
    Ok(Prepared { source, content })
}
