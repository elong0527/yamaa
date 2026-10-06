//! Owned schema service over bounded decoded-tree requests, without YAML or filesystem IO.
mod errors;
pub(crate) mod wire;

use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::{fmt, io, panic::catch_unwind};
use wire::Tree;
use yamaa_core::schema::{
    BundleLimits, NormalizationBudget, NormalizationLimits, NormalizedDocument, SchemaAliasKind,
    SchemaContext, SchemaDiagnostic, SchemaModule, SchemaShape, SchemaSource, SchemaStructure,
    ValidationBudget, ValidationLimits, DIAGNOSTIC_UNICODE_VERSION,
};

pub const MAX_REQUEST_BYTES: usize = 8_388_608;
pub const MAX_RESPONSE_BYTES: usize = 16_777_216;
const MAX_QUERIES: usize = 256;
const PROTOCOL: &str = "schema/1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportError {
    RequestLimit,
    ResponseLimit,
    InvalidRequest,
    UnsupportedProtocol,
    InvalidDocument,
    InvalidQuery,
    Internal,
}
impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::RequestLimit => "schema request exceeds byte limit",
            Self::ResponseLimit => "schema response exceeds byte limit",
            Self::InvalidRequest => "invalid schema request",
            Self::UnsupportedProtocol => "unsupported schema protocol",
            Self::InvalidDocument => "invalid decoded schema document",
            Self::InvalidQuery => "invalid schema query",
            Self::Internal => "internal schema failure",
        })
    }
}
impl std::error::Error for TransportError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Module {
    name: String,
    document: Tree,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    modules: Vec<Module>,
    entry: usize,
    root_class: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompileRequest {
    protocol: String,
    schema: Bundle,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnalyzeRequest {
    protocol: String,
    queries: Vec<Query>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchRequest {
    protocol: String,
    schema: Bundle,
    queries: Vec<Query>,
}
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum Query {
    ComposeLayers {
        layers: Vec<Tree>,
    },
    ValidateTypes {
        types: Vec<String>,
        document: Tree,
        fragment: bool,
        path: String,
    },
    NormalizeTypes {
        types: Vec<String>,
        document: Tree,
        fragment: bool,
    },
    MatchingTypes {
        types: Vec<String>,
        document: Tree,
        fragment: bool,
    },
    ValidateDocument {
        document: Tree,
    },
    NormalizeDocument {
        document: Tree,
    },
    ExpandWindows {
        document: Tree,
        strict: bool,
    },
    ValidateDescriptor {
        descriptor: usize,
        document: Tree,
        fragment: bool,
        path: String,
    },
    NormalizeDescriptor {
        descriptor: usize,
        document: Tree,
        fragment: bool,
    },
    MatchingMember {
        descriptor: usize,
        document: Tree,
        fragment: bool,
    },
}

/// An admitted immutable snapshot; no request buffers or host objects survive compilation.
pub struct CompiledSchema {
    schema: SchemaStructure,
}

fn decode<T: DeserializeOwned>(request: &str) -> Result<T, TransportError> {
    if request.len() > MAX_REQUEST_BYTES {
        return Err(TransportError::RequestLimit);
    }
    serde_json::from_str(request).map_err(|_| TransportError::InvalidRequest)
}
fn protocol(value: &str) -> Result<(), TransportError> {
    if value == PROTOCOL {
        Ok(())
    } else {
        Err(TransportError::UnsupportedProtocol)
    }
}

struct Response(Vec<u8>);
struct ResponseSize(usize);
impl io::Write for ResponseSize {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|size| *size <= MAX_RESPONSE_BYTES)
            .ok_or_else(|| io::Error::other("response limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl ResponseSize {
    fn charge(&mut self, value: &Value) -> Result<(), TransportError> {
        serde_json::to_writer(self, value).map_err(|error| {
            if error.is_io() {
                TransportError::ResponseLimit
            } else {
                TransportError::Internal
            }
        })
    }
}
impl io::Write for Response {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_RESPONSE_BYTES {
            return Err(io::Error::other("response limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn encode(outcome: Value) -> Result<String, TransportError> {
    let mut output = Response(Vec::new());
    serde_json::to_writer(&mut output, &json!({"protocol":PROTOCOL,"outcome":outcome})).map_err(
        |error| {
            if error.is_io() {
                TransportError::ResponseLimit
            } else {
                TransportError::Internal
            }
        },
    )?;
    String::from_utf8(output.0).map_err(|_| TransportError::Internal)
}

fn compile(bundle: Bundle) -> Result<(Option<CompiledSchema>, Value), TransportError> {
    let mut modules = Vec::new();
    if bundle.modules.len() > BundleLimits::default().modules {
        return Ok((
            None,
            errors::limit("bundle", "modules", BundleLimits::default().modules),
        ));
    }
    for module in bundle.modules {
        let document = match module.document.admit()? {
            Ok(document) => document,
            Err(error) => return errors::document(error).map(|outcome| (None, outcome)),
        };
        modules.push(SchemaModule {
            name: module.name,
            document,
        });
    }
    let mut schema = match SchemaStructure::admit(
        modules,
        bundle.entry,
        &bundle.root_class,
        BundleLimits::default(),
    ) {
        Ok(schema) => schema,
        Err(error) => return errors::bundle(error).map(|outcome| (None, outcome)),
    };
    match schema.prepare_defaults(&mut ValidationBudget::new(ValidationLimits::default())) {
        Ok(defaults) if !defaults.is_empty() => {
            return Ok((
                None,
                json!({"status":"invalid_defaults","schema":metadata(&schema),"defaults":defaults.iter().map(|failure| json!({"descriptor":failure.descriptor,"diagnostics":diagnostics(&failure.diagnostics)})).collect::<Vec<_>>()}),
            ))
        }
        Err(error) => return errors::validation(error).map(|outcome| (None, outcome)),
        _ => {}
    }
    let outcome = metadata(&schema);
    Ok((Some(CompiledSchema { schema }), outcome))
}

/// Compile and validate defaults before a host can retain a prepared schema service.
pub fn compile_schema(request: &str) -> Result<(Option<CompiledSchema>, String), TransportError> {
    catch_unwind(|| {
        let request: CompileRequest = decode(request)?;
        protocol(&request.protocol)?;
        let (schema, outcome) = compile(request.schema)?;
        Ok((schema, encode(outcome)?))
    })
    .map_err(|_| TransportError::Internal)?
}

/// Stateless host entry point; all operations in a batch share request-level budgets.
pub fn interpret_schema(request: &str) -> Result<String, TransportError> {
    catch_unwind(|| {
        let request: BatchRequest = decode(request)?;
        protocol(&request.protocol)?;
        if request.queries.len() > MAX_QUERIES {
            return encode(errors::limit("query", "queries", MAX_QUERIES));
        }
        let (schema, outcome) = compile(request.schema)?;
        let Some(schema) = schema else {
            return encode(outcome);
        };
        let mut analyzed = schema.run(request.queries)?;
        analyzed["schema"] = outcome;
        encode(analyzed)
    })
    .map_err(|_| TransportError::Internal)?
}

impl CompiledSchema {
    /// Each request starts fresh policies; query branches cannot reset their shared accounting.
    pub fn analyze(&self, request: &str) -> Result<String, TransportError> {
        catch_unwind(|| {
            let request: AnalyzeRequest = decode(request)?;
            protocol(&request.protocol)?;
            if request.queries.len() > MAX_QUERIES {
                return encode(errors::limit("query", "queries", MAX_QUERIES));
            }
            encode(self.run(request.queries)?)
        })
        .map_err(|_| TransportError::Internal)?
    }
    fn run(&self, queries: Vec<Query>) -> Result<Value, TransportError> {
        let mut budget = NormalizationBudget::new(NormalizationLimits::default());
        let mut outcomes = Vec::with_capacity(queries.len());
        // Bound retained batch outcomes incrementally, before retaining another result.
        // Final encoding independently accounts for the envelope and metadata.
        let mut response_size = ResponseSize(0);
        for query in queries {
            if let Query::ComposeLayers { layers } = query {
                let mut documents = Vec::with_capacity(layers.len());
                let mut defect = None;
                for layer in layers {
                    match layer.admit()? {
                        Ok(document) => documents.push(document),
                        Err(error) => {
                            defect = Some(errors::document(error)?);
                            break;
                        }
                    }
                }
                let outcome = if let Some(defect) = defect {
                    defect
                } else {
                    match self.schema.compose_layers(&documents, &mut budget) {
                        Ok(composed) => json!({
                            "status":"composed", "document":Tree::from_core(&composed.document),
                            "provenance":composed.provenance.iter().map(|origin| json!({
                                "path":origin.path, "layer":origin.layer,
                            })).collect::<Vec<_>>(),
                        }),
                        Err(error) => {
                            let mut outcome = errors::normalization(error.error)?;
                            if let Some(document) = error.context_document {
                                outcome["context_document"] =
                                    serde_json::to_value(Tree::from_core(&document))
                                        .map_err(|_| TransportError::Internal)?;
                            }
                            outcome
                        }
                    }
                };
                response_size.charge(&outcome)?;
                outcomes.push(outcome);
                continue;
            }
            if let Query::ExpandWindows { document, strict } = query {
                let outcome = match document.admit()? {
                    Ok(input) => match self
                        .schema
                        .expand_named_windows(&input, strict, &mut budget)
                    {
                        Ok(expanded) => json!({
                            "status":"expanded", "document":Tree::from_core(&expanded.document),
                            "origins":expanded.origins,
                            "references":expanded.references.iter().map(|reference| json!({
                                "path":reference.path, "definition":reference.definition,
                            })).collect::<Vec<_>>(),
                        }),
                        Err(error) => errors::normalization(error)?,
                    },
                    Err(error) => errors::document(error)?,
                };
                response_size.charge(&outcome)?;
                outcomes.push(outcome);
                continue;
            }
            let (tree, operation, descriptor, fragment, path, raw_types) = match query {
                Query::ExpandWindows { .. } | Query::ComposeLayers { .. } => {
                    unreachable!("handled above")
                }
                Query::ValidateDocument { document } => {
                    (document, 0, None, false, String::new(), None)
                }
                Query::NormalizeDocument { document } => {
                    (document, 1, None, false, String::new(), None)
                }
                Query::ValidateDescriptor {
                    descriptor,
                    document,
                    fragment,
                    path,
                } => (document, 2, Some(descriptor), fragment, path, None),
                Query::NormalizeDescriptor {
                    descriptor,
                    document,
                    fragment,
                } => (document, 3, Some(descriptor), fragment, String::new(), None),
                Query::MatchingMember {
                    descriptor,
                    document,
                    fragment,
                } => (document, 4, Some(descriptor), fragment, String::new(), None),
                Query::ValidateTypes {
                    types,
                    document,
                    fragment,
                    path,
                } => (document, 5, None, fragment, path, Some(types)),
                Query::NormalizeTypes {
                    types,
                    document,
                    fragment,
                } => (document, 6, None, fragment, String::new(), Some(types)),
                Query::MatchingTypes {
                    types,
                    document,
                    fragment,
                } => (document, 7, None, fragment, String::new(), Some(types)),
            };
            if descriptor.is_some_and(|d| d >= self.schema.descriptors().len()) {
                return Err(TransportError::InvalidQuery);
            }
            let types = if let Some(types) = raw_types {
                match self
                    .schema
                    .parse_query_types(&types, budget.validation_scope())
                {
                    Ok(types) => types,
                    Err(error) => {
                        let outcome = errors::validation(error)?;
                        response_size.charge(&outcome)?;
                        outcomes.push(outcome);
                        continue;
                    }
                }
            } else {
                Vec::new()
            };
            let input = match tree.admit()? {
                Ok(document) => document,
                Err(error) => {
                    let outcome = errors::document(error)?;
                    response_size.charge(&outcome)?;
                    outcomes.push(outcome);
                    continue;
                }
            };
            let outcome = match operation {
                0 | 2 => {
                    let result = if operation == 0 {
                        self.schema
                            .validate_document(&input, budget.validation_scope())
                    } else {
                        self.schema.validate_descriptor(
                            descriptor.unwrap(),
                            &input,
                            input.root(),
                            &path,
                            fragment,
                            budget.validation_scope(),
                        )
                    };
                    match result {
                        Ok(findings) => {
                            json!({"status":if findings.is_empty() {"valid"} else {"invalid"},"diagnostics":diagnostics(&findings)})
                        }
                        Err(error) => errors::validation(error)?,
                    }
                }
                1 | 3 => {
                    let result = if operation == 1 {
                        self.schema.normalize_document(&input, &mut budget)
                    } else {
                        self.schema.normalize_descriptor(
                            descriptor.unwrap(),
                            &input,
                            input.root(),
                            fragment,
                            &mut budget,
                        )
                    };
                    match result {
                        Ok(document) => normalized(document),
                        Err(error) => errors::normalization(error)?,
                    }
                }
                4 => match self.schema.matching_member(
                    descriptor.unwrap(),
                    &input,
                    input.root(),
                    fragment,
                    budget.validation_scope(),
                ) {
                    Ok(member) => json!({"status":"matched","member":member}),
                    Err(error) => errors::validation(error)?,
                },
                5 => match self.schema.validate_types(
                    &types,
                    &input,
                    input.root(),
                    &path,
                    fragment,
                    budget.validation_scope(),
                ) {
                    Ok(findings) => {
                        json!({"status":if findings.is_empty() {"valid"} else {"invalid"},"diagnostics":diagnostics(&findings)})
                    }
                    Err(error) => errors::validation(error)?,
                },
                6 => match self.schema.normalize_types(
                    &types,
                    &input,
                    input.root(),
                    fragment,
                    &mut budget,
                ) {
                    Ok(document) => normalized(document),
                    Err(error) => errors::normalization(error)?,
                },
                7 => match self.schema.matching_types(
                    &types,
                    &input,
                    input.root(),
                    fragment,
                    budget.validation_scope(),
                ) {
                    Ok(member) => json!({"status":"matched","member":member}),
                    Err(error) => errors::validation(error)?,
                },
                _ => unreachable!(),
            };
            response_size.charge(&outcome)?;
            outcomes.push(outcome);
        }
        Ok(json!({"status":"analyzed","results":outcomes}))
    }
}

fn shape(shape: &SchemaShape) -> Value {
    match shape {
        SchemaShape::Class(fields) => {
            json!({"kind":"class","fields":fields.iter().map(|f| json!({"name":f.name,"descriptor":f.descriptor})).collect::<Vec<_>>()})
        }
        SchemaShape::Descriptor(descriptor) => json!({"kind":"descriptor","descriptor":descriptor}),
    }
}
fn metadata(schema: &SchemaStructure) -> Value {
    json!({"status":"compiled","version":schema.version(),"root_class":schema.root_class().name,
        "diagnostic_unicode_version":DIAGNOSTIC_UNICODE_VERSION,
        "modules":schema.modules().iter().map(|m| &m.name).collect::<Vec<_>>(),
        "classes":schema.classes().iter().map(|class| json!({"name":class.name,"fields":class.fields.iter().map(|f| json!({"name":f.name,"descriptor":f.descriptor})).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "aliases":schema.aliases().iter().map(|alias| match alias.kind {
            SchemaAliasKind::Descriptor(descriptor) => json!({"name":alias.name,"descriptor":descriptor}),
            SchemaAliasKind::Registry(registry) => json!({"name":alias.name,"registry":registry}),
        }).collect::<Vec<_>>(),
        "registries":schema.registries().iter().map(|registry| json!({"name":registry.name,"entries":registry.entries.iter().map(|entry|json!({"name":entry.name,"shape":shape(&entry.shape)})).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "descriptors":schema.descriptors().iter().map(|located| {
            let d = &located.descriptor;
            json!({"module":located.module,"source_node":located.source_node,"path":located.path,"type":d.members().iter().map(|m|m.node_text(m.root()).unwrap()).collect::<Vec<_>>(),"required":d.required(),"default_node":d.default_node(),"values":d.permitted(),"pattern":d.pattern().map(|(text,_)|text),"min_length":d.minimum(),"size":d.size()})
        }).collect::<Vec<_>>()})
}
fn diagnostics(findings: &[SchemaDiagnostic]) -> Vec<Value> {
    findings.iter().map(|finding| json!({"path":finding.path,"condition":finding.condition,"requirement":finding.requirement,
        "context":finding.context.iter().map(|(name,value)| json!({"name":name,"value":match value {
            SchemaContext::Text(value) => json!({"kind":"text","value":value}),
            SchemaContext::Count(value) => json!({"kind":"count","value":value}),
            SchemaContext::Null => json!({"kind":"null"}),
            SchemaContext::InputValue(node) => json!({"kind":"input_value","node":node}),
            SchemaContext::DescriptorValues(descriptor) => json!({"kind":"descriptor_values","descriptor":descriptor}),
            SchemaContext::DescriptorPattern(descriptor) => json!({"kind":"descriptor_pattern","descriptor":descriptor}),
            SchemaContext::DescriptorMinimum(descriptor) => json!({"kind":"descriptor_minimum","descriptor":descriptor}),
            SchemaContext::DescriptorSize(descriptor) => json!({"kind":"descriptor_size","descriptor":descriptor}),
        }})).collect::<Vec<_>>() })).collect()
}
fn normalized(value: NormalizedDocument) -> Value {
    json!({"status":"normalized","document":Tree::from_core(&value.document),"origins":value.origins.iter().map(|origin| {
        let source = match origin.source { SchemaSource::Input => json!({"kind":"input"}), SchemaSource::Default { module, descriptor } => json!({"kind":"default","module":module,"descriptor":descriptor}) };
        json!({"source":source,"node":origin.node,"generated":origin.generated})
    }).collect::<Vec<_>>()})
}
