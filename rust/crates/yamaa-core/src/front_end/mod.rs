//! Front ends: each lowers an admitted document into the [`crate::ast`] tree
//! that every analysis reads, through the construct lowering in
//! [`lowering`]. A specification is read only in 1.0 ([`v1::spec`]).

pub mod lowering;
pub mod v1;

/// The 0.5 project, environment, function and codelist files, which a 0.5
/// project still links until its route goes (P2-13d).
pub mod v0_5 {
    pub mod project;
}
