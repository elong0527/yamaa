//! Lower admitted 1.0 documents into the [`crate::ast`] tree.
//!
//! Every public call reads 1.0 files through this front end (#3030 D76). A
//! project's own files lower into their roots in [`project`], and a
//! specification into its tree in [`spec`].

pub mod project;
pub mod spec;
