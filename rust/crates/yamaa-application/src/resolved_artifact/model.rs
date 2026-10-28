//! Versioned inspection facts. Deserialization grants no execution or IO authority.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use yamaa_core::json::{self, Value};
use yamaa_core::schema::{Document, DocumentNode as N};

pub const FORMAT: &str = "yamaa.resolved-specification";
pub const ARTIFACT_VERSION: u32 = 1;
pub const MODEL_KIND: &str = "normalized_with_named_dictionary_references";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldOrigin {
    pub declaring_source: Option<String>,
    /// The one-based line and column at which the declaring layer wrote the
    /// value, as its reader located the node: present together, on an
    /// authored origin only. Version 1 gained them as optional fields, so an
    /// origin an earlier engine wrote without them still reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    pub source_kind: OriginKind,
    pub generated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginKind {
    Authored,
    SchemaDefault,
    Composed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceRole {
    InputData,
    ProducerSchema,
    OutputData,
    Report,
    MappingDictionary,
    ParentSpecification,
    IncludedSpecification,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelativeTo {
    Declaring,
    Entry,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Location {
    Local { path: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub spec_path: String,
    pub role: ResourceRole,
    pub owning_entry: String,
    pub declaring_source: String,
    pub relative_to: RelativeTo,
    pub base_source: String,
    pub written: String,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub entry: String,
    /// The language version the entry's resolved specification records in
    /// `yamaa`, the newest any of its layers declares (P-VER-1).
    pub language_version: String,
    #[serde(
        serialize_with = "crate::json_wire::serialize_value",
        deserialize_with = "crate::json_wire::deserialize_value"
    )]
    pub specification: Value,
    /// JSON object keys are sorted; this ledger preserves authored mapping order.
    pub mapping_order: BTreeMap<String, Vec<String>>,
    /// RFC 6901 pointers address every value, including generated containers.
    pub origins: BTreeMap<String, FieldOrigin>,
    /// Composition layers in postorder (one entry for standalone documents).
    pub layers: Vec<String>,
    pub resources: Vec<Resource>,
}

/// The JSON wire format is intentionally distinct from an authored specification.
/// Its only replay operation is inspection: compilation accepts original sources.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedArtifact {
    format: String,
    artifact_version: u32,
    model_kind: String,
    replay: String,
    entry: String,
    project: Option<String>,
    entries: Vec<Entry>,
}

#[derive(Debug)]
pub enum Error {
    Json(serde_json::Error),
    Version,
    Facts(&'static str),
    MappingKey,
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid resolved specification JSON: {error}"),
            Self::Version => formatter.write_str(
                "unsupported resolved specification format, version, model kind or replay policy",
            ),
            Self::Facts(reason) => {
                write!(formatter, "invalid resolved specification facts: {reason}")
            }
            Self::MappingKey => {
                formatter.write_str("normalized specification contains a non-text mapping key")
            }
        }
    }
}
impl std::error::Error for Error {}

impl ResolvedArtifact {
    pub fn new(entry: String, project: Option<String>, entries: Vec<Entry>) -> Result<Self, Error> {
        let artifact = Self {
            format: FORMAT.into(),
            artifact_version: ARTIFACT_VERSION,
            model_kind: MODEL_KIND.into(),
            replay: "inspection_only".into(),
            entry,
            project,
            entries,
        };
        artifact.validate()?;
        Ok(artifact)
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn entry(&self) -> &str {
        &self.entry
    }
    pub fn project(&self) -> Option<&str> {
        self.project.as_deref()
    }
    /// Deterministic compact UTF-8 JSON; exact integer tokens never use binary64.
    pub fn to_json(&self) -> Result<String, Error> {
        Ok(serde_json::to_string(self)?)
    }
    /// The artifact as one owned tree, exactly as its JSON text reads, so
    /// wide numbers keep their spelling.
    pub fn to_value(&self) -> Result<Value, Error> {
        Ok(crate::json_wire::decode(&self.to_json()?)?)
    }
    /// Validate the distinct format without opening sources, resolving locations,
    /// compiling expressions, invoking callbacks or changing the stored facts.
    pub fn from_json(text: &str) -> Result<Self, Error> {
        let artifact: Self = serde_json::from_str(text)?;
        artifact.validate()?;
        Ok(artifact)
    }
    fn validate(&self) -> Result<(), Error> {
        if self.format != FORMAT
            || self.artifact_version != ARTIFACT_VERSION
            || self.model_kind != MODEL_KIND
            || self.replay != "inspection_only"
        {
            return Err(Error::Version);
        }
        // A linked producing specification is a resource of the requested
        // entry, never an entry of its own.
        if self.entries.len() != 1 || self.entries[0].entry != self.entry {
            return Err(Error::Facts(
                "the artifact holds exactly the requested entry",
            ));
        }
        for entry in &self.entries {
            // The entry names its language version as its specification
            // records it.
            let version = &entry.language_version;
            if entry.entry.is_empty()
                || version.is_empty()
                || !entry.specification.is_object()
                || !entry.layers.contains(&entry.entry)
                || !entry.origins.contains_key("")
            {
                return Err(Error::Facts("incomplete entry"));
            }
            if entry.specification.get("yamaa").and_then(Value::as_str) != Some(version) {
                return Err(Error::Facts("language version differs from the model"));
            }
            validate_ledgers(entry)?;
            for resource in &entry.resources {
                let expected = match resource.relative_to {
                    RelativeTo::Entry => &entry.entry,
                    RelativeTo::Declaring => &resource.declaring_source,
                };
                if resource.owning_entry != entry.entry
                    || &resource.base_source != expected
                    || resource.written.is_empty()
                    || resource.declaring_source.is_empty()
                {
                    return Err(Error::Facts("inconsistent resource provenance"));
                }
                let Location::Local { path } = &resource.location;
                if path.is_empty() {
                    return Err(Error::Facts("empty resource location"));
                }
            }
        }
        Ok(())
    }
}

fn validate_ledgers(entry: &Entry) -> Result<(), Error> {
    let mut values = BTreeSet::new();
    let mut mappings = BTreeSet::new();
    let mut pending = vec![(String::new(), &entry.specification)];
    while let Some((pointer, value)) = pending.pop() {
        values.insert(pointer.clone());
        let origin = entry
            .origins
            .get(&pointer)
            .ok_or(Error::Facts("missing value origin"))?;
        match origin.source_kind {
            OriginKind::Composed if origin.declaring_source.is_some() => {
                return Err(Error::Facts("composed origin claims one declaring source"))
            }
            OriginKind::Authored | OriginKind::SchemaDefault
                if origin
                    .declaring_source
                    .as_ref()
                    .is_none_or(String::is_empty) =>
            {
                return Err(Error::Facts("origin source is absent"))
            }
            _ => {}
        }
        // A location is a written one: a line and a column, both one-based,
        // where a layer wrote the value.
        match (origin.line, origin.column) {
            (None, None) => {}
            (Some(line), Some(column))
                if origin.source_kind == OriginKind::Authored && line > 0 && column > 0 => {}
            _ => return Err(Error::Facts("origin location is not a written one")),
        }
        match value {
            Value::Object(fields) => {
                mappings.insert(pointer.clone());
                let order = entry
                    .mapping_order
                    .get(&pointer)
                    .ok_or(Error::Facts("missing mapping order"))?;
                let keys: BTreeSet<_> = order.iter().collect();
                if keys.len() != order.len()
                    || keys.len() != fields.len()
                    || keys.iter().any(|key| !fields.contains_key(key.as_str()))
                {
                    return Err(Error::Facts("mapping order differs from model keys"));
                }
                for (key, child) in fields {
                    pending.push((pointer_child(&pointer, key), child));
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    pending.push((pointer_child(&pointer, &index.to_string()), child));
                }
            }
            _ => {}
        }
    }
    if values.len() != entry.origins.len() || mappings.len() != entry.mapping_order.len() {
        return Err(Error::Facts(
            "ledger contains pointers absent from the model",
        ));
    }
    Ok(())
}

pub fn pointer_child(parent: &str, key: &str) -> String {
    format!("{parent}/{}", key.replace('~', "~0").replace('/', "~1"))
}

/// Project the admitted model, retaining exact numbers and mapping iteration order.
/// This is representation only; schema admission and normalization precede it.
pub fn specification_value(
    document: &Document,
) -> Result<(Value, BTreeMap<String, Vec<String>>), Error> {
    fn visit(
        document: &Document,
        node: usize,
        pointer: &str,
        order: &mut BTreeMap<String, Vec<String>>,
    ) -> Result<Value, Error> {
        Ok(match &document.nodes()[node] {
            N::Null => Value::Null,
            N::Boolean(value) => Value::Bool(*value),
            N::Integer(value) => Value::Number(
                value
                    .parse()
                    .map_err(|_| Error::Facts("invalid integer occurrence"))?,
            ),
            N::Float(value) => Value::Number(
                json::Number::from_f64(*value).ok_or(Error::Facts("nonfinite float occurrence"))?,
            ),
            N::Text(value) => Value::String(value.clone()),
            N::Sequence(items) => Value::Array(
                items
                    .iter()
                    .enumerate()
                    .map(|(index, &child)| {
                        visit(
                            document,
                            child,
                            &pointer_child(pointer, &index.to_string()),
                            order,
                        )
                    })
                    .collect::<Result<_, _>>()?,
            ),
            N::Mapping(items) => {
                let mut keys = Vec::new();
                let mut mapping = yamaa_core::json::Map::new();
                for &(key, child) in items {
                    let N::Text(key) = &document.nodes()[key] else {
                        return Err(Error::MappingKey);
                    };
                    keys.push(key.clone());
                    mapping.insert(
                        key.clone(),
                        visit(document, child, &pointer_child(pointer, key), order)?,
                    );
                }
                order.insert(pointer.into(), keys);
                Value::Object(mapping)
            }
        })
    }
    let mut order = BTreeMap::new();
    Ok((visit(document, document.root(), "", &mut order)?, order))
}
