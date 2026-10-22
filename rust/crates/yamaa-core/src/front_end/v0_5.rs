//! Lower 0.5 documents into the AST: the 0.5 front end's entry points into
//! the lowering both surfaces share ([`crate::front_end::lowering`]), which
//! reads the 0.5 spelling where the two differ ([`Surface::V0_5`]).
//!
//! A spec lowers once, after composition and pruning ([`lower`]); pruning
//! lowers the composed document before that, only to find what each
//! declaration reads ([`lower_document`]). A project's own files lower into
//! their roots in [`project`], and its study and sections in [`sections`].

pub mod project;
#[cfg(test)]
pub(crate) mod tests;

use crate::ast::Surface;
use crate::front_end::lowering::{lower_surface, Lowered, Positions};
use crate::schema::{Document, SpecificationDocument};

/// Lower one admitted spec. Without positions every span is
/// [`crate::ast::Span::UNKNOWN`].
pub fn lower(spec: &SpecificationDocument, positions: Option<&Positions>) -> Lowered {
    lower_document(spec.document(), positions)
}

/// Lower a document the model has not admitted yet, such as a composed
/// spec before pruning (LAYER-11 precedes requiredness, LAYER-12). Lowering
/// stays total: a missing required field lowers to an unsupported node, and
/// final validation still reports it.
pub fn lower_document(d: &Document, positions: Option<&Positions>) -> Lowered {
    lower_surface(d, positions, Surface::V0_5)
}

/// The study and sections of a 0.5 project, which lower as a 1.0 project's
/// do ([`crate::front_end::lowering::sections`]).
pub mod sections {
    use crate::ast::{SubmissionDeclarations, Surface};
    use crate::front_end::lowering::project::Metas;
    use crate::front_end::lowering::sections::{lower, Error};
    use crate::schema::Document;

    /// Lower the study and sections of a whole 0.5 project document, as
    /// [`super::project`] does, into an arena of their own.
    pub fn sections(input: &Document) -> Result<SubmissionDeclarations, Error<'_>> {
        let mut paths = crate::ast::Paths::new();
        let mut metas = Metas::new(None, &mut paths);
        let meta = metas.root(input.root());
        lower(input, input.root(), &mut metas, meta, Surface::V0_5)
    }
}
