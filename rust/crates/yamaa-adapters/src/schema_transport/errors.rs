//! Stable outcomes keep language findings, policy refusal and transport defects separate.
use super::{diagnostics, TransportError};
use serde_json::{json, Value};
use yamaa_core::{regex, schema::*};

pub(super) fn limit(phase: &str, resource: &str, limit: usize) -> Value {
    json!({"status":"resource_limit","phase":phase,"resource":resource,"limit":limit})
}
fn regex_resource(resource: regex::Resource) -> &'static str {
    use regex::Resource::*;
    match resource {
        PatternBytes => "pattern_bytes",
        Nodes => "nodes",
        Groups => "groups",
        Depth => "depth",
        Repetition => "repetition",
        Width => "width",
        WidthWork => "width_work",
        WidthCells => "width_cells",
        SubjectBytes => "subject_bytes",
        Work => "work",
        StateCells => "state_cells",
    }
}
fn document_resource(resource: DocumentResource) -> &'static str {
    match resource {
        DocumentResource::Nodes => "nodes",
        DocumentResource::TextBytes => "text_bytes",
        DocumentResource::Edges => "edges",
        DocumentResource::Depth => "depth",
    }
}
pub(super) fn document(error: DocumentError) -> Result<Value, TransportError> {
    match error {
        DocumentError::Limit {
            resource,
            limit: maximum,
        } => Ok(limit(
            "decoded_document",
            document_resource(resource),
            maximum,
        )),
        _ => Err(TransportError::InvalidDocument),
    }
}
pub(super) fn validation(error: ValidationError) -> Result<Value, TransportError> {
    Ok(match error {
        ValidationError::InvalidDescriptor => return Err(TransportError::InvalidQuery),
        ValidationError::Depth { limit: maximum } => limit("validation", "depth", maximum),
        ValidationError::Diagnostics { limit: maximum } => {
            limit("validation", "diagnostics", maximum)
        }
        ValidationError::DiagnosticText { limit: maximum } => {
            limit("validation", "diagnostic_text_bytes", maximum)
        }
        ValidationError::Constraint(ConstraintError::Work { limit: maximum }) => {
            limit("validation", "work", maximum)
        }
        ValidationError::Constraint(ConstraintError::Regex(error)) => {
            limit("regex_match", regex_resource(error.resource), error.limit)
        }
    })
}
pub(super) fn normalization(error: NormalizationError) -> Result<Value, TransportError> {
    Ok(match error {
        NormalizationError::Validation(error) => return validation(error),
        NormalizationError::Invalid(findings) => {
            json!({"status":"invalid","diagnostics":diagnostics(&findings)})
        }
        NormalizationError::Defaults(defaults) => {
            json!({"status":"invalid_defaults","defaults":defaults.iter().map(|failure|json!({"descriptor":failure.descriptor,"diagnostics":diagnostics(&failure.diagnostics)})).collect::<Vec<_>>()})
        }
        NormalizationError::Limit {
            resource,
            limit: maximum,
        } => limit(
            "normalization",
            match resource {
                NormalizationResource::Nodes => "nodes",
                NormalizationResource::TextBytes => "text_bytes",
                NormalizationResource::Edges => "edges",
                NormalizationResource::Depth => "depth",
            },
            maximum,
        ),
        NormalizationError::Document(DocumentError::Limit {
            resource,
            limit: maximum,
        }) => limit("normalized_document", document_resource(resource), maximum),
        NormalizationError::Document(DocumentError::NonScalarKey) => {
            json!({"status":"unsupported","feature":"normalized_mapping_key"})
        }
        NormalizationError::Document(_) => return Err(TransportError::Internal),
    })
}
pub(super) fn bundle(error: BundleError) -> Result<Value, TransportError> {
    Ok(match error {
        BundleError::Invalid(issues) => {
            json!({"status":"invalid_schema","issues":issues.into_iter().map(|issue|json!({"module":issue.module,"path":issue.path,"issue":issue_kind(issue.kind)})).collect::<Vec<_>>()})
        }
        BundleError::Limit {
            resource,
            limit: maximum,
        } => limit(
            "bundle",
            match resource {
                BundleResource::Modules => "modules",
                BundleResource::InputNodes => "input_nodes",
                BundleResource::InputTextBytes => "input_text_bytes",
                BundleResource::ExpandedFields => "expanded_fields",
                BundleResource::Work => "work",
                BundleResource::Depth => "depth",
            },
            maximum,
        ),
        BundleError::Descriptor(DescriptorError::InvalidNode) => {
            return Err(TransportError::Internal)
        }
        BundleError::Descriptor(DescriptorError::Limit {
            resource,
            limit: maximum,
        }) => limit(
            "descriptor",
            match resource {
                DescriptorResource::Descriptors => "descriptors",
                DescriptorResource::InputNodes => "input_nodes",
                DescriptorResource::InputTextBytes => "input_text_bytes",
                DescriptorResource::TypeBytes => "type_bytes",
                DescriptorResource::PatternBytes => "pattern_bytes",
            },
            maximum,
        ),
        BundleError::Descriptor(DescriptorError::TypeLimit(TypeError::Limit {
            resource,
            limit: maximum,
        })) => limit(
            "type",
            match resource {
                TypeResource::Bytes => "bytes",
                TypeResource::Nodes => "nodes",
                TypeResource::Depth => "depth",
            },
            maximum,
        ),
        BundleError::Descriptor(DescriptorError::TypeLimit(TypeError::Invalid { .. })) => {
            return Err(TransportError::Internal)
        }
        BundleError::Descriptor(DescriptorError::Regex(error)) => match error {
            regex::CompileError::Limit {
                resource,
                limit: maximum,
            } => limit("regex_compile", regex_resource(resource), maximum),
            regex::CompileError::Unsupported { byte, feature } => {
                json!({"status":"unsupported","phase":"regex_compile","byte":byte,"feature":feature})
            }
            regex::CompileError::Invalid { .. } => return Err(TransportError::Internal),
        },
    })
}
fn issue_kind(issue: BundleIssueKind) -> Value {
    use BundleIssueKind::*;
    match issue {
        Descriptor(issue) => json!({"code":"descriptor","issue":descriptor_issue(issue)}),
        ModuleName => json!({"code":"module_name"}),
        DuplicateModule(name) => json!({"code":"duplicate_module","name":name}),
        ExpectedMapping => json!({"code":"expected_mapping"}),
        Version => json!({"code":"version"}),
        VersionMismatch { expected, actual } => {
            json!({"code":"version_mismatch","expected":expected,"actual":actual})
        }
        IncludesList => json!({"code":"includes_list"}),
        UnsafeInclude { node } => json!({"code":"unsafe_include","node":node}),
        MissingInclude(name) => json!({"code":"missing_include","name":name}),
        IncludeCycle(names) => json!({"code":"include_cycle","names":names}),
        UnusedModule => json!({"code":"unused_module"}),
        DeclarationName => json!({"code":"declaration_name"}),
        UnknownDeclaration(name) => json!({"code":"unknown_declaration","name":name}),
        DuplicateDeclaration(name) => json!({"code":"duplicate_declaration","name":name}),
        DuplicateRegistryEntry(name) => json!({"code":"duplicate_registry_entry","name":name}),
        MissingRoot(name) => json!({"code":"missing_root","name":name}),
        FieldsFrom => json!({"code":"fields_from"}),
        UnknownFieldsFrom(name) => json!({"code":"unknown_fields_from","name":name}),
        FieldsFromCycle(names) => json!({"code":"fields_from_cycle","names":names}),
        FieldEntry => json!({"code":"field_entry"}),
        FieldName => json!({"code":"field_name"}),
        DuplicateField(name) => json!({"code":"duplicate_field","name":name}),
        RegistryOnly => json!({"code":"registry_only"}),
        RegistryName => json!({"code":"registry_name"}),
        EmptyRegistry => json!({"code":"empty_registry"}),
        UnreferencedRegistry => json!({"code":"unreferenced_registry"}),
        RegistryEntryName => json!({"code":"registry_entry_name"}),
        RegistryEntryShape => json!({"code":"registry_entry_shape"}),
        UnknownRegistry(name) => json!({"code":"unknown_registry","name":name}),
        UnknownType(name) => json!({"code":"unknown_type","name":name}),
    }
}
fn descriptor_issue(issue: DescriptorIssue) -> Value {
    use DescriptorIssue::*;
    match issue {
        ExpectedMapping => json!({"code":"expected_mapping"}),
        UnknownKeyword { key } => json!({"code":"unknown_keyword","key":key}),
        MissingType => json!({"code":"missing_type"}),
        InvalidTypeValue => json!({"code":"invalid_type_value"}),
        TypeSyntax { member, byte } => json!({"code":"type_syntax","member":member,"byte":byte}),
        RequiredBoolean => json!({"code":"required_boolean"}),
        RequiredDefault => json!({"code":"required_default"}),
        Description => json!({"code":"description"}),
        PatternRequiresString => json!({"code":"pattern_requires_string"}),
        PatternText => json!({"code":"pattern_text"}),
        InvalidPattern {
            pattern,
            byte,
            reason,
        } => json!({"code":"invalid_pattern","pattern":pattern,"byte":byte,"reason":reason}),
        MinimumRequiresString => json!({"code":"minimum_requires_string"}),
        MinimumNonnegative => json!({"code":"minimum_nonnegative"}),
        SizeRequiresCollection => json!({"code":"size_requires_collection"}),
        SizeNonnegative => json!({"code":"size_nonnegative"}),
        ValuesRequiresString => json!({"code":"values_requires_string"}),
        ValuesTextSequence => json!({"code":"values_text_sequence"}),
    }
}
