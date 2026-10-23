//! The front end: it lowers an admitted 1.0 document into the [`crate::ast`]
//! tree that every analysis reads. Each file type's entry point is in [`v1`],
//! and the construct lowering they share in [`lowering`].

pub mod lowering;
pub mod v1;
