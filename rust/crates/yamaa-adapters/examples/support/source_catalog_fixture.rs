//! Catalog controls reuse native normalization and the published-contract
//! component. A focused contract observation never admits a filesystem graph.
use serde_json::{json, Map, Value};
use yamaa_adapters::{issue_rows::Issue, yaml_decode::decode_yaml};
use yamaa_core::{
    diagnostic::Diagnostic,
    producer_contract,
    schema::{DocumentNode, SpecificationDocument, ValidationBudget},
    specification::{PreparedSpecification, SourceDeclaration, SourceProfile},
    table::TableSchema,
    value::ColumnType,
};
use yamaa_engine::{
    source_ingestion,
    source_metadata::{self, SourceSchema},
};

fn document(value: &Value) -> SpecificationDocument {
    let source = value
        .as_str()
        .expect("original ordered normalized document");
    let decoded =
        decode_yaml(source.as_bytes(), Default::default()).expect("authored normalized document");
    SpecificationDocument::admit(
        decoded.document,
        &mut ValidationBudget::new(Default::default()),
    )
    .expect("bounded normalized model")
    .expect("complete normalized model")
}
fn kind(value: &str) -> ColumnType {
    match value {
        "str" => ColumnType::Str,
        "int" => ColumnType::Int,
        "float" => ColumnType::Float,
        "date" => ColumnType::Date,
        "datetime" => ColumnType::DateTime,
        _ => panic!("closed native column kind"),
    }
}
fn row(diagnostic: Diagnostic) -> Value {
    let mut row = Issue::from_core(diagnostic)
        .expect("portable metadata diagnostic")
        .as_value();
    row["context"] =
        serde_json::from_str(row["context"].as_str().unwrap()).expect("native canonical context");
    row
}
fn fields(schema: &TableSchema) -> Value {
    Value::Object(
        schema
            .columns()
            .iter()
            .map(|field| {
                let kind = match field.kind {
                    ColumnType::Str => "str",
                    ColumnType::Int => "int",
                    ColumnType::Float => "float",
                    ColumnType::Date => "date",
                    ColumnType::DateTime => "datetime",
                };
                (field.name.clone(), json!(kind))
            })
            .collect(),
    )
}

pub fn observe(request: Value) -> Value {
    let specification = document(&request["specification"]);
    if let Some(published) = request["published"].as_object().filter(|v| !v.is_empty()) {
        let mut catalog = Map::new();
        let mut issues = Vec::new();
        for (dataset, producer) in published {
            let producer = document(producer);
            let contract = producer_contract::prepare(&producer, Default::default())
                .expect("admitted actual producer publication contract");
            let carrier = specification.document();
            let input = carrier
                .field(carrier.root(), "input")
                .expect("normalized input");
            let source = carrier.field(input, dataset).expect("authored dataset");
            let text = |name| {
                carrier
                    .field(source, name)
                    .and_then(|index| match &carrier.nodes()[index] {
                        DocumentNode::Text(text) => Some(text.clone()),
                        _ => None,
                    })
            };
            let path = text("path").expect("authored path operand");
            let types =
                carrier
                    .field(source, "types")
                    .map_or_else(Vec::new, |index| match &carrier.nodes()[index] {
                        DocumentNode::Mapping(items) => items
                            .iter()
                            .map(|&(name, value)| {
                                let DocumentNode::Text(name) = &carrier.nodes()[name] else {
                                    panic!("normalized field name")
                                };
                                let DocumentNode::Text(value) = &carrier.nodes()[value] else {
                                    panic!("normalized closed field kind")
                                };
                                (name.clone(), kind(value))
                            })
                            .collect(),
                        DocumentNode::Null => Vec::new(),
                        _ => panic!("normalized optional type carrier"),
                    });
            let declaration = SourceDeclaration {
                name: dataset.clone(),
                path: path.clone(),
                types,
                profile: SourceProfile::from_path(&path).expect("authored closed profile"),
                encoding: None,
                empty_string_present: false,
                ordinal: text("ordinal"),
            };
            let physical = TableSchema::new(
                contract
                    .fields()
                    .iter()
                    .map(|field| yamaa_core::table::Column {
                        name: field.name.clone(),
                        kind: field.kind,
                    })
                    .collect(),
            )
            .expect("native published fields");
            match source_ingestion::ordinal(
                &declaration,
                &physical,
                0,
                source_ingestion::Limits {
                    columns: source_metadata::Limits::default().columns,
                    cells: 0,
                },
            ) {
                Ok(Some(ordinal)) => {
                    catalog.insert(dataset.clone(), fields(ordinal.schema()));
                }
                Ok(None) => {
                    catalog.insert(dataset.clone(), fields(&physical));
                }
                Err(finding) => issues.push(row(finding
                    .finding(
                        dataset,
                        declaration.ordinal.as_deref().expect("authored ordinal"),
                    )
                    .expect("registered ordinal refusal")
                    .diagnostic_with_limit(yamaa_core::diagnostic::MAX_PROJECTION_BYTES)
                    .expect("portable ordinal refusal"))),
            }
        }
        return json!({"scope":"published-source component; no graph admission", "catalog":catalog, "issues":issues});
    }
    let prepared = PreparedSpecification::prepare(&specification)
        .expect("admitted actual normalized declaration");
    let sources = request["sources"]
        .as_array()
        .expect("complete actual physical carriers")
        .iter()
        .map(|source| {
            let dataset = source[0].as_str().expect("actual dataset").to_owned();
            let columns = source[1]
                .as_array()
                .expect("actual ordered columns")
                .iter()
                .map(|field| yamaa_core::table::Column {
                    name: field[0].as_str().expect("actual field").to_owned(),
                    kind: kind(field[1].as_str().expect("actual native kind")),
                })
                .collect();
            SourceSchema::new(dataset, columns).expect("bounded closed carrier")
        })
        .collect::<Vec<_>>();
    match source_metadata::normalized_schemas(&prepared, &sources, Default::default())
        .expect("complete original metadata admission")
    {
        Ok(schemas) => json!({"catalog":sources.iter().zip(schemas).map(|(source,schema)|
            (source.dataset.clone(),fields(&schema))).collect::<Map<_,_>>(),"issues":[]}),
        Err(issues) => {
            json!({"catalog":null,"issues":issues.into_iter().map(row).collect::<Vec<_>>()})
        }
    }
}
