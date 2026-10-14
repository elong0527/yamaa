//! Read-only projections of held schema/document facts for repository checks.
//! Admission, normalization and diagnostics remain in the shared core/engine.
use crate::{
    issue_rows::Issue,
    specification_source::{CapturedSchema, Limits, Source},
};
use serde_json::{json, Value};
use std::sync::Arc;
use yamaa_core::schema::{
    admit_schema_diagnostics, Document, DocumentNode, NormalizationBudget, SchemaDiagnostic,
    SchemaDiagnosticBudget, SchemaDiagnosticValue, ValidationBudget,
};

pub struct Schema(pub Arc<CapturedSchema>);

/// Schema authoring defects retain their original core-owned kind and location.
/// They are repository structure findings, not fabricated language issue rows.
pub fn capture_failure(error: &crate::specification_source::Error) -> String {
    use yamaa_core::schema::BundleError;
    if let crate::specification_source::Error::Bundle(BundleError::Invalid(defects)) = error {
        return json!({"schema_defects":defects.iter().map(|defect|json!({
            "module":defect.module,"path":defect.path,"kind":format!("{:?}",defect.kind)
        })).collect::<Vec<_>>()})
        .to_string();
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
        let defaults = schema
            .validate_defaults(&mut ValidationBudget::new(Default::default()))
            .map_err(|_| refused())?;
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
        let input =
            crate::yaml_decode::decode_yaml(source, Default::default()).map_err(|_| refused())?;
        let schema = self.0.structure();
        let mut budget = ValidationBudget::new(Default::default());
        let types = schema
            .parse_query_types(types, &mut budget)
            .map_err(|_| refused())?;
        let findings = schema
            .validate_types(
                &types,
                &input.document,
                input.document.root(),
                "",
                fragment,
                &mut budget,
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
        let input =
            crate::yaml_decode::decode_yaml(source, Default::default()).map_err(|_| refused())?;
        let schema = self.0.structure();
        let types = schema
            .parse_query_types(types, &mut ValidationBudget::new(Default::default()))
            .map_err(|_| refused())?;
        let normalized = schema
            .normalize_types(
                &types,
                &input.document,
                input.document.root(),
                fragment,
                &mut NormalizationBudget::new(Default::default()),
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
