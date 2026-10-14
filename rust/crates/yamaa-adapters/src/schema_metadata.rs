//! Read-only projections of held schema/document facts for repository checks.
//! Admission, normalization and diagnostics remain in the shared core/engine.
use crate::{
    issue_rows::Issue,
    specification_source::{CapturedSchema, Limits, Source},
};
use serde_json::{json, Value};
use std::sync::Arc;
use yamaa_core::schema::{
    admit_schema_diagnostics, Document, DocumentNode, SchemaDiagnostic, SchemaDiagnosticBudget,
    SchemaDiagnosticValue,
};

pub struct Schema(pub Arc<CapturedSchema>);

/// Host query-carrier capacity admission belongs to the adapter, before copies.
#[derive(Default)]
pub struct Query {
    values: Vec<String>,
    bytes: usize,
}
impl Query {
    pub fn push(&mut self, value: &str) -> Result<(), String> {
        if self.values.len() >= 128 {
            return Err(refused());
        }
        self.bytes = self
            .bytes
            .checked_add(value.len())
            .filter(|&size| size <= 65_536)
            .ok_or_else(refused)?;
        self.values.push(value.into());
        Ok(())
    }
    pub fn into_values(self) -> Vec<String> {
        self.values
    }
}

fn admit_queries(types: &[String]) -> Result<(), String> {
    let mut query = Query::default();
    for value in types {
        query.push(value)?;
    }
    Ok(())
}

/// Native YAML discovery facts grant no filesystem authority or schema admission.
pub fn includes(bytes: &[u8], identity: &str) -> Result<Vec<String>, String> {
    let limits = Limits::default();
    if bytes.len() > limits.captured_bytes || identity.len() > limits.identity_bytes {
        return Err(refused());
    }
    let input = crate::yaml_decode::decode_yaml(bytes, Default::default()).map_err(|error| {
        crate::specification_diagnostics::capture_failure(
            &crate::specification_source::Error::Decode {
                identity: identity.into(),
                error,
            },
        )
    })?;
    let Some(index) = input.document.field(input.document.root(), "includes") else {
        return Ok(Vec::new());
    };
    let DocumentNode::Sequence(entries) = &input.document.nodes()[index] else {
        return Ok(Vec::new());
    };
    Ok(entries
        .iter()
        .filter_map(|&entry| match &input.document.nodes()[entry] {
            DocumentNode::Text(value) => Some(value.clone()),
            _ => None,
        })
        .collect())
}

/// Schema authoring defects retain their original core-owned kind and location.
/// They are repository structure findings, not fabricated language issue rows.
pub fn capture_failure(error: &crate::specification_source::Error, sources: &[Source]) -> String {
    use yamaa_core::schema::{BundleError, BundleIssueKind, DescriptorIssue};
    if let crate::specification_source::Error::Bundle(BundleError::Invalid(defects)) = error {
        let mut projected = Vec::new();
        let mut remaining = crate::specification_check::MAX_ISSUE_BYTES;
        for defect in defects {
            let source = match sources.get(defect.module) {
                Some(source) => source,
                None => return refused(),
            };
            let key = match defect.kind {
                BundleIssueKind::Descriptor(DescriptorIssue::UnknownKeyword { key }) => Some(key),
                BundleIssueKind::UnsafeInclude { node } => Some(node),
                _ => None,
            };
            let authored = if let Some(index) = key {
                let decoded =
                    match crate::yaml_decode::decode_yaml(&source.bytes, Default::default()) {
                        Ok(decoded) => decoded,
                        Err(_) => return refused(),
                    };
                decoded
                    .document
                    .nodes()
                    .get(index)
                    .and_then(yamaa_core::schema::scalar_diagnostic_label)
            } else {
                None
            };
            let item = json!({"module":defect.module,"identity":source.identity,
                "path":defect.path,"kind":format!("{:?}",defect.kind),"authored":authored});
            remaining = match remaining.checked_sub(item.to_string().len().saturating_add(1)) {
                Some(remaining) => remaining,
                None => return refused(),
            };
            projected.push(item);
        }
        let encoded = json!({"schema_defects":projected}).to_string();
        return if encoded.len() <= crate::specification_check::MAX_ISSUE_BYTES {
            encoded
        } else {
            refused()
        };
    }
    crate::specification_diagnostics::capture_failure(error)
}

fn refused() -> String {
    "native schema metadata projection refused".into()
}

/// Encode scalar kinds explicitly, preserving arbitrary integer text and keys.
/// This is a representation of an admitted arena, not another normalization pass.
pub fn document(document: &Document) -> Result<String, String> {
    let nodes = document
        .nodes()
        .iter()
        .map(|node| match node {
            DocumentNode::Null => json!(["null"]),
            DocumentNode::Boolean(value) => json!(["bool", value]),
            DocumentNode::Integer(value) => json!(["int", value]),
            DocumentNode::Float(value) => json!(["float", value]),
            DocumentNode::Text(value) => json!(["str", value]),
            DocumentNode::Sequence(values) => json!(["list", values]),
            DocumentNode::Mapping(values) => json!(["dict", values]),
        })
        .collect::<Vec<_>>();
    let encoded = json!({"nodes": nodes, "root": document.root()}).to_string();
    if encoded.len() > Limits::default().captured_bytes {
        return Err(refused());
    }
    Ok(encoded)
}

fn issues(
    schema: &CapturedSchema,
    document: Option<&Document>,
    findings: &[SchemaDiagnostic],
) -> Result<Vec<Issue>, String> {
    let projected = admit_schema_diagnostics(
        schema.structure(),
        document,
        findings,
        &[],
        &mut SchemaDiagnosticBudget::new(),
    )
    .map_err(|_| refused())?;
    let mut result = Vec::new();
    for finding in projected.iter() {
        let context = finding
            .context
            .map(|(name, value)| {
                let value = match value {
                    SchemaDiagnosticValue::Text(value) => json!(value),
                    SchemaDiagnosticValue::Integer(value) => {
                        Value::Number(value.parse().map_err(|_| refused())?)
                    }
                    SchemaDiagnosticValue::Count(value) => json!(value),
                    SchemaDiagnosticValue::Null => Value::Null,
                    SchemaDiagnosticValue::Boolean(value) => json!(value),
                    SchemaDiagnosticValue::Float(value) => json!(value),
                    SchemaDiagnosticValue::TextList(value) => json!(value),
                };
                Ok((name.to_owned(), value))
            })
            .collect::<Result<serde_json::Map<_, _>, String>>()?;
        result.push(
            Issue::from_diagnostic(json!({"phase":finding.phase, "condition":finding.condition,
            "requirement":finding.requirement, "spec_paths":[finding.path], "context":context}))
            .map_err(|_| refused())?,
        );
    }
    if !crate::issue_rows::within_limit(&result, crate::specification_check::MAX_ISSUE_BYTES) {
        return Err(refused());
    }
    Ok(result)
}

impl Schema {
    pub fn version(&self) -> &str {
        self.0.structure().version()
    }
    pub fn admit(&self, identity: &str, bytes: &[u8]) -> Result<String, String> {
        let limits = Limits::default();
        if identity.len() > limits.identity_bytes || bytes.len() > limits.captured_bytes {
            return Err(refused());
        }
        let source = Source {
            identity: identity.into(),
            bytes: bytes.into(),
        };
        let schema = self.0.structure();
        let prepared = if schema
            .root_class()
            .fields
            .iter()
            .any(|field| field.name == "schema_version")
        {
            self.0.prepare_structural(source)
        } else {
            self.0.prepare_class(source)
        }
        .map_err(|error| crate::specification_diagnostics::capture_failure(&error))?;
        document(&prepared.normalized().document)
    }
    pub fn fields(&self, name: &str) -> Result<Vec<String>, String> {
        let class = self.0.structure().class_named(name).ok_or_else(refused)?;
        Ok(class
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect())
    }
    pub fn defaults(&self) -> Result<Vec<Issue>, String> {
        let schema = self.0.structure();
        let defaults =
            yamaa_engine::schema_capture::default_findings(schema).map_err(|_| refused())?;
        let mut result = Vec::new();
        let mut remaining = crate::specification_check::MAX_ISSUE_BYTES.saturating_sub(2);
        for default in defaults {
            let located = &schema.descriptors()[default.descriptor];
            let projected = issues(
                &self.0,
                Some(&schema.modules()[located.module].document),
                &default.diagnostics,
            )?;
            for issue in projected {
                let bytes = issue
                    .as_value()
                    .to_string()
                    .len()
                    .saturating_add(usize::from(!result.is_empty()));
                remaining = remaining.checked_sub(bytes).ok_or_else(refused)?;
                result.push(issue);
            }
        }
        if !crate::issue_rows::within_limit(&result, crate::specification_check::MAX_ISSUE_BYTES) {
            return Err(refused());
        }
        Ok(result)
    }
    /// Query an explicitly named type union against captured YAML bytes.
    pub fn validate(
        &self,
        source: &[u8],
        types: &[String],
        fragment: bool,
    ) -> Result<Vec<Issue>, String> {
        if source.len() > Limits::default().captured_bytes {
            return Err(refused());
        }
        admit_queries(types)?;
        let input =
            crate::yaml_decode::decode_yaml(source, Default::default()).map_err(|_| refused())?;
        let findings = yamaa_engine::schema_capture::validate_query(
            self.0.structure(),
            &input.document,
            types,
            fragment,
        )
        .map_err(|_| refused())?;
        issues(&self.0, Some(&input.document), &findings)
    }
    pub fn normalize(
        &self,
        source: &[u8],
        types: &[String],
        fragment: bool,
    ) -> Result<String, String> {
        if source.len() > Limits::default().captured_bytes {
            return Err(refused());
        }
        admit_queries(types)?;
        let input =
            crate::yaml_decode::decode_yaml(source, Default::default()).map_err(|_| refused())?;
        let normalized = yamaa_engine::schema_capture::normalize_query(
            self.0.structure(),
            &input.document,
            types,
            fragment,
        )
        .map_err(|_| refused())?;
        document(&normalized.document)
    }
}

/// Entry selection has already admitted the held document and its provenance.
pub fn selected_document(selected: &crate::file_workflow::Selected) -> Result<String, String> {
    use yamaa_engine::build::Prepared;
    let prepared = match selected {
        Prepared::Plain(owner) => owner.run().document(),
        Prepared::Ordinary(owner) => owner.run().document(),
        Prepared::Producer(owner) => {
            let graph = owner.graph();
            &graph.nodes()[graph.checked().metadata().root()].document
        }
    };
    document(prepared.model().document())
}
