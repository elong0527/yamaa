//! Front ends: each lowers one surface's admitted document into the
//! [`crate::ast`] tree that every analysis reads, through the construct
//! lowering both surfaces share ([`lowering`]).

pub mod lowering;
pub mod v0_5;
pub mod v1;
