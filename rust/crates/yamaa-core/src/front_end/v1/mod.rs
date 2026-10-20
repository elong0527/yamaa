//! Lower admitted 1.0 documents into the [`crate::ast`] tree.
//!
//! Until the engine reads 1.0 files (#3030 D76, P1-9) only tests reach this
//! front end. A project's own files lower into their roots in [`project`],
//! and a specification lowers into the nodes of its 0.5 counterpart in [`spec`].

pub mod project;
pub mod spec;
