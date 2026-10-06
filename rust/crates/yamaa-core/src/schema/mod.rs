//! Shared schema interpretation over decoded documents; no YAML or filesystem IO.

mod bundle;
mod constraints;
mod descriptor;
mod document;
mod type_expression;

pub use bundle::{
    BundleError, BundleIssue, BundleIssueKind, BundleLimits, BundleResource, LocatedDescriptor,
    SchemaAlias, SchemaAliasKind, SchemaClass, SchemaField, SchemaModule, SchemaRegistry,
    SchemaRegistryEntry, SchemaShape, SchemaStructure,
};
pub use constraints::{ConstraintBudget, ConstraintError, ConstraintUsage, ConstraintViolation};
pub use descriptor::{
    Descriptor, DescriptorBudget, DescriptorError, DescriptorIssue, DescriptorLimits,
    DescriptorReport, DescriptorResource, DescriptorUsage,
};
pub use document::{Document, DocumentError, DocumentLimits, DocumentNode, DocumentResource};
pub use type_expression::{TypeError, TypeExpression, TypeLimits, TypeNode, TypeResource};
