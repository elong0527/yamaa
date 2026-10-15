//! Bounded metadata capture for the study inventory; accepts no study-data port.
use alloc::{string::String, vec::Vec};
use yamaa_core::{
    schema::Document,
    submission_inventory::{self as core, Inventory},
};
pub trait Ports {
    type Specification;
    type Error;
    fn capture(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Self::Specification, Self::Error>;
    fn document(specification: &Self::Specification) -> &Document;
    fn identity(specification: &Self::Specification) -> &str;
}
#[derive(Debug)]
pub enum Error<E> {
    Port { written: String, error: E },
    Metadata(core::Error),
    Limit,
}
pub struct Captured<S> {
    pub inventory: Inventory,
    pub specifications: Vec<S>,
}
pub fn capture<P: Ports>(
    environment: &Document,
    declaring: &str,
    port: &mut P,
) -> Result<Captured<P::Specification>, Error<P::Error>> {
    let references = core::references(environment, Default::default()).map_err(Error::Metadata)?;
    if references.len() > crate::define::METADATA_LIMITS.snapshots {
        return Err(Error::Limit);
    }
    let mut specifications = Vec::new();
    let mut nodes = 0usize;
    let mut text = 0usize;
    for written in references {
        let specification = port
            .capture(declaring, written)
            .map_err(|error| Error::Port {
                written: written.into(),
                error,
            })?;
        nodes = nodes
            .checked_add(P::document(&specification).nodes().len())
            .filter(|&nodes| nodes <= 262_144)
            .ok_or(Error::Limit)?;
        text = text
            .checked_add(P::identity(&specification).len())
            .filter(|&text| text <= crate::define::METADATA_LIMITS.text)
            .ok_or(Error::Limit)?;
        specifications.push(specification);
    }
    let documents = specifications.iter().map(P::document).collect::<Vec<_>>();
    let inventory =
        Inventory::from_specifications(&documents, Default::default()).map_err(Error::Metadata)?;
    Ok(Captured {
        inventory,
        specifications,
    })
}
