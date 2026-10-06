//! Shared schema interpretation over decoded documents; no YAML or filesystem IO.

mod bundle;
mod constraints;
mod descriptor;
mod document;
mod normalization;
mod type_expression;
mod validation;

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
pub use normalization::{
    NormalizationBudget, NormalizationError, NormalizationLimits, NormalizationResource,
    NormalizedDocument, SchemaOrigin, SchemaSource,
};
pub use type_expression::{TypeError, TypeExpression, TypeLimits, TypeNode, TypeResource};
pub use validation::{
    DefaultDiagnostics, SchemaContext, SchemaDiagnostic, ValidationBudget, ValidationError,
    ValidationLimits,
};
