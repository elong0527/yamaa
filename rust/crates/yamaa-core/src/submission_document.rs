//! Closed submission declaration metadata from one normalized environment.
//! Borrowed locations and closed owned values grant no execution or file authority.
use crate::{
    project_environment::Submission,
    schema::{Document, DocumentNode as N},
};

mod decode;
mod model;
pub use decode::decode;
pub use model::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cause {
    Shape {
        node: usize,
    },
    /// The complete admitted structural set, never a fitting prefix.
    Findings(Vec<Finding>),
}
#[derive(Debug, thiserror::Error)]
#[error("submission document: cause={cause:?}")]
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
pub fn locate(document: &Document, root: usize) -> Result<Locations<'_>, Error<'_>> {
    let refuse = |cause| Error {
        input: document,
        cause,
    };
    let Some(N::Mapping(fields)) = document.nodes().get(root) else {
        return Err(refuse(Cause::Shape { node: root }));
    };
    for &(key, _) in fields {
        if !matches!(document.nodes().get(key), Some(N::Text(_))) {
            return Err(refuse(Cause::Shape { node: key }));
        }
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
