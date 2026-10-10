//! Bounded submission declaration metadata from one normalized environment.
//! Borrowed locations and closed owned values grant no execution or file authority.
use crate::{
    project_environment::Submission,
    schema::{Document, DocumentNode as N},
};
use alloc::vec::Vec;

mod decode;
mod model;
pub use decode::decode;
pub use model::*;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub fields: usize,
    pub declarations: usize,
    pub work: usize,
    pub nodes: usize,
    pub depth: usize,
    pub list_items: usize,
    pub text_bytes: usize,
    /// Conservative complete escaped-text capacity, before any projection.
    pub escaped_bytes: usize,
    pub path_segments: usize,
    pub findings: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            fields: 65_536,
            declarations: 4,
            work: 67_108_864,
            nodes: 131_072,
            depth: 64,
            list_items: 65_536,
            text_bytes: 8_388_608,
            escaped_bytes: 83_886_080,
            path_segments: 393_216,
            findings: 65_536,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Fields,
    Declarations,
    Work,
    Nodes,
    Depth,
    ListItems,
    TextBytes,
    EscapedBytes,
    PathSegments,
    Findings,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cause {
    Shape {
        node: usize,
    },
    Limit(Resource),
    /// The complete admitted structural set, never a fitting prefix.
    Findings(Vec<Finding>),
}
#[derive(Debug)]
pub struct Error<'a> {
    pub input: &'a Document,
    pub cause: Cause,
}
/// Original node facts stay paired with their exact source document. They are
/// not a validated submission model or an accepted Define-XML capability.
#[derive(Debug)]
pub struct Locations<'a> {
    input: &'a Document,
    study: Option<usize>,
    sections: Vec<(Submission, usize)>,
}
impl<'a> Locations<'a> {
    pub fn input(&self) -> &'a Document {
        self.input
    }
    pub fn study(&self) -> Option<usize> {
        self.study
    }
    pub fn sections(&self) -> &[(Submission, usize)] {
        &self.sections
    }
}

/// Admit the complete root-field scan before lookups or owned location growth.
/// No scalar text is cloned; selected sections retain authored mapping order.
pub fn locate(
    document: &Document,
    root: usize,
    mut limits: Limits,
) -> Result<Locations<'_>, Error<'_>> {
    let refuse = |cause| Error {
        input: document,
        cause,
    };
    let Some(N::Mapping(fields)) = document.nodes().get(root) else {
        return Err(refuse(Cause::Shape { node: root }));
    };
    limits.fields = limits
        .fields
        .checked_sub(fields.len())
        .ok_or_else(|| refuse(Cause::Limit(Resource::Fields)))?;
    limits.work = fields
        .len()
        .checked_mul(8)
        .and_then(|work| limits.work.checked_sub(work))
        .ok_or_else(|| refuse(Cause::Limit(Resource::Work)))?;
    // Charge every key's comparison work before the selection scan begins.
    for &(key, _) in fields {
        let Some(N::Text(text)) = document.nodes().get(key) else {
            return Err(refuse(Cause::Shape { node: key }));
        };
        limits.work = text
            .len()
            .checked_mul(4)
            .and_then(|work| limits.work.checked_sub(work))
            .ok_or_else(|| refuse(Cause::Limit(Resource::Work)))?;
    }
    let mut study = None;
    let mut selected = [None; 3];
    let mut count = 0;
    for &(key, node) in fields {
        let Some(N::Text(text)) = document.nodes().get(key) else {
            unreachable!("admitted immutable key")
        };
        let section = match text.as_str() {
            "sdtm" => Some(Submission::Sdtm),
            "adam" => Some(Submission::Adam),
            "send" => Some(Submission::Send),
            _ => None,
        };
        if text == "study" || section.is_some() {
            limits.declarations = limits
                .declarations
                .checked_sub(1)
                .ok_or_else(|| refuse(Cause::Limit(Resource::Declarations)))?;
            if let Some(section) = section {
                let slot = selected
                    .get_mut(count)
                    .ok_or_else(|| refuse(Cause::Shape { node }))?;
                *slot = Some((section, node));
                count += 1;
            } else {
                study = Some(node);
            }
        }
    }
    // Every declaration is admitted before the first owned location buffer.
    let mut sections = Vec::with_capacity(count);
    sections.extend(selected.into_iter().flatten());
    Ok(Locations {
        input: document,
        study,
        sections,
    })
}
