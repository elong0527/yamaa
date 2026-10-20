//! The 1.0 front end for specifications (#3030 P1-5, D30).
//!
//! A 1.0 specification is decoded, each layer admitted at the version it
//! declares and the layers composed with their `clear` lists (P1-3, P1-4),
//! pruned and ordered on its lowering (LAYER-11, LAYER-13), and every named
//! window of the resolved specification expanded
//! ([`crate::inheritance_preparation::prepare_versioned`]). This module
//! lowers that resolved specification into the same [`crate::ast`] nodes as
//! its 0.5 counterpart, so every analysis and the binder read one tree:
//!
//! - an expression field holds text in the closed grammar or a YAML scalar,
//!   where a bare name is a reference and quoted text a literal (D18, D31);
//! - the renamed operands, handlers and bindings of the approved rename
//!   table (D58) lower to the nodes their 0.5 spellings lower to, with
//!   `closed` (P-ORD-2), `origins` (P-ORIGIN-1), and `baseline_flag`'s
//!   `window.order_by` to the order its `tie_break` gave;
//! - an input's `key` lowers as declared, which its implicit joins and
//!   keyless qualified aggregates match on (D23, D63);
//! - `model`, a dataset's `standard`, several origins and a value-level
//!   comparator are stored for the pull requests that check or build them
//!   (P1-10, P1-13, P1-14); until then the switch refuses what is not built
//!   (P1-9).
//!
//! Every node carries the position of the deepest node its layer wrote
//! (P-ISSUE-1), and paths render in the 1.0 notation (P-CLR-2).
//!
//! The construct lowering is shared with [`crate::front_end::v0_5`], which
//! reads the spelling of the surface it was started for where the two differ.

pub mod checks;

use crate::ast::Surface;
use crate::language::LanguageVersion;
use crate::schema::{Document, DocumentNode as N};

pub use crate::front_end::v0_5::{Lowered, Positions};

/// Lower one resolved 1.0 specification. Without positions every span is
/// [`crate::ast::Span::UNKNOWN`].
pub fn lower(resolved: &Document, positions: Option<&Positions>) -> Lowered {
    crate::front_end::v0_5::lower_surface(resolved, positions, surface(resolved))
}

/// The surface a 1.x document's `yamaa` declares; a document that declares
/// no version it can be read at lowers as 1.0, and its own validation
/// reports the header.
pub fn surface(document: &Document) -> Surface {
    let minor = document
        .field(document.root(), "yamaa")
        .and_then(|node| match &document.nodes()[node] {
            N::Text(version) => LanguageVersion::parse(version),
            _ => None,
        })
        .map_or(0, LanguageVersion::minor);
    Surface::V1 { minor }
}
