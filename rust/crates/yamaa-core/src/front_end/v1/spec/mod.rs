//! The 1.0 front end for specifications (#3030 P1-5, D30).
//!
//! A 1.0 specification is decoded, each layer admitted at the version it
//! declares and the layers composed with their `clear` lists (P1-3, P1-4),
//! pruned and ordered on its lowering (LAYER-11, LAYER-13), and every named
//! window of the resolved specification expanded
//! ([`crate::inheritance_preparation::prepare_versioned`]). This module
//! lowers that resolved specification into the [`crate::ast`] nodes every
//! analysis and the binder read:
//!
//! - an expression field holds text in the closed grammar or a YAML scalar,
//!   where a bare name is a reference and quoted text a literal (D18, D31);
//! - the operands, handlers and bindings of the approved rename table (D58)
//!   lower to their nodes, with `closed` (P-ORD-2), `origins` (P-ORIGIN-1),
//!   and `baseline_flag`'s `window.order_by` to its date and tie-break terms;
//! - an input's `key` lowers as declared, which its implicit joins and
//!   keyless qualified aggregates match on (D23, D63);
//! - `model` lowers as declared, which [`checks`] requires beside submission
//!   metadata and the submission rules read (SUB-3, SUB-23);
//! - a dataset's `standard`, a column's several origins and a value-level
//!   comparator lower as written, which the section's standards (DEFINE-6),
//!   the declared origins (SUB-19) and the value-level entries (SUB-12) read.
//!
//! Every node carries the position of the deepest node its layer wrote
//! (P-ISSUE-1), and paths render in the 1.0 notation (P-CLR-2). The construct
//! lowering lives in [`crate::front_end::lowering`].

pub mod checks;

use crate::language::LanguageVersion;
use crate::schema::{Document, DocumentNode as N};

pub use crate::front_end::lowering::{Lowered, Positions};

/// Lower one resolved 1.0 specification. Without positions every span is
/// [`crate::ast::Span::UNKNOWN`].
pub fn lower(resolved: &Document, positions: Option<&Positions>) -> Lowered {
    crate::front_end::lowering::lower_specification(resolved, positions, minor(resolved))
}

/// The minor number of the version a 1.x document's `yamaa` declares; a
/// document that declares no version it can be read at lowers as 1.0, and
/// its own validation reports the header.
fn minor(document: &Document) -> u32 {
    document
        .field(document.root(), "yamaa")
        .and_then(|node| match &document.nodes()[node] {
            N::Text(version) => LanguageVersion::parse(version),
            _ => None,
        })
        .map_or(0, LanguageVersion::minor)
}
