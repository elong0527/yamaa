//! Shared schema interpretation over decoded documents; no YAML or filesystem IO.

mod bundle;
mod composition;
mod constraints;
mod descriptor;
mod document;
mod inheritance_dependencies;
mod inheritance_references;
mod layer_admission;
mod layer_materialization;
mod layers;
mod model;
mod normalization;
mod path_render;
mod printable_data;
mod type_expression;
mod validation;
mod windows;

pub use bundle::{
    BundleError, BundleIssue, BundleIssueKind, BundleLimits, BundleResource, LocatedDescriptor,
    SchemaAlias, SchemaAliasKind, SchemaClass, SchemaField, SchemaModule, SchemaRegistry,
    SchemaRegistryEntry, SchemaShape, SchemaStructure,
};
pub use composition::{ComposedValue, CompositionOrigin};
pub use constraints::{ConstraintBudget, ConstraintError, ConstraintUsage, ConstraintViolation};
pub use descriptor::{
    Descriptor, DescriptorBudget, DescriptorError, DescriptorIssue, DescriptorLimits,
    DescriptorReport, DescriptorResource, DescriptorUsage,
};
pub use document::{
    Document, DocumentError, DocumentLimits, DocumentNode, DocumentResource, ScalarKey,
};
pub use inheritance_dependencies::{InheritanceDependencyError, InheritanceDependencyIssue};
pub use inheritance_references::{
    InheritanceReference, InheritanceReferenceError, InheritanceReferenceKind,
};
pub use layer_materialization::LayerCompositionError;
pub use layers::{ComposedLayers, LayerProvenance};
pub use model::SpecificationDocument;
pub use normalization::{
    NormalizationBudget, NormalizationError, NormalizationLimits, NormalizationResource,
    NormalizedDocument, SchemaOrigin, SchemaSource,
};
pub use path_render::{scalar_diagnostic_label, DIAGNOSTIC_UNICODE_VERSION};
pub use type_expression::{TypeError, TypeExpression, TypeLimits, TypeNode, TypeResource};
pub use validation::{
    DefaultDiagnostics, SchemaContext, SchemaDiagnostic, ValidationBudget, ValidationError,
    ValidationLimits,
};

pub use windows::{ExpandedWindows, WindowReference};
