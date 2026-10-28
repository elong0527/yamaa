//! Shared schema interpretation over decoded documents; no YAML or filesystem IO.

mod bundle;
mod composition;
mod constraints;
mod descriptor;
mod diagnostic_projection;
mod dictionary_resources;
mod document;
mod layers;
mod normalization;
mod path_render;
mod printable_data;
mod spec_path;
mod type_expression;
mod validation;
mod versioned_composition;
mod versioned_layer;
mod windows;

pub use bundle::safe_include as schema_include_name;
pub use bundle::{
    BundleError, BundleIssue, BundleIssueKind, BundleResource, LocatedDescriptor, SchemaAlias,
    SchemaAliasKind, SchemaClass, SchemaField, SchemaModule, SchemaRegistry, SchemaRegistryEntry,
    SchemaShape, SchemaStructure,
};
pub use composition::{ComposedValue, CompositionOrigin};
pub use constraints::ConstraintViolation;
pub use descriptor::{
    Descriptor, DescriptorError, DescriptorIssue, DescriptorReport, DescriptorSite, FIELD_KINDS,
};
pub use diagnostic_projection::{
    admit_schema_diagnostics, schema_diagnostic_kind_label, ResolvedSchemaDiagnostic,
    SchemaDiagnosticContext, SchemaDiagnosticError, SchemaDiagnosticReport, SchemaDiagnosticValue,
};
pub use dictionary_resources::{AdmittedMappingDictionary, MappingDictionaryReference};
pub(crate) use document::canonical_integer;
pub use document::{
    Document, DocumentError, DocumentKind, DocumentNode, DocumentResource, ScalarKey,
};
pub use layers::{ComposedLayers, LayerNodeOrigin, LayerProvenance};
pub use normalization::{
    NormalizationError, NormalizationResource, NormalizedDocument, SchemaOrigin, SchemaSource,
};
pub use path_render::{
    quoted_diagnostic_text, scalar_diagnostic_label, DIAGNOSTIC_UNICODE_VERSION,
};
pub(crate) use spec_path::{read_step, SpecPath};
pub use type_expression::{TypeError, TypeExpression, TypeNode, TypeResource};
pub use validation::{
    DefaultDiagnostics, SchemaContext, SchemaDiagnostic, SiteRole, ValidationError, WrittenSite,
};
pub use versioned_composition::VersionedCompositionError;
pub(crate) use versioned_composition::{shorthand_key, written_path};
pub use versioned_layer::BLOCKS;

pub use windows::{ExpandedWindows, WindowReference};
