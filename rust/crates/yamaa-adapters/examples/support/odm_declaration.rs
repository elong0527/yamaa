//! Typed fixture projections reuse production preparation and diagnostic representation.
use serde_json::{json, Value};
use yamaa_adapters::{
    shipped_schema, specification_diagnostics,
    specification_run::{Error, PreparedRun},
    specification_source::Source,
};
use yamaa_core::{
    odm_source::{self, StoredField},
    specification::SourceInterpretation,
    table::{Column, TableSchema},
    value::ColumnType,
};

pub fn observe(source: &str, headers: &[Vec<String>]) -> Value {
    let document = match shipped_schema::capture()
        .expect("captured shipped schema")
        .prepare_standalone(Source {
            identity: "fixture.yaml".into(),
            bytes: source.as_bytes().into(),
        }) {
        Ok(document) => document,
        Err(error) => {
            return json!({"status":"structural_refusal", "error":format!("{error:?}"), "fields":odm_source::FIELDS})
        }
    };
    let run = match PreparedRun::prepare(document) {
        Ok(run) => run,
        Err(error) => {
            return json!({"status":"declaration_refusal", "issues":specification_diagnostics::findings(&error,None), "fields":odm_source::FIELDS})
        }
    };
    assert_eq!(
        headers.len(),
        run.compiled().sources().len(),
        "authored header count"
    );
    let mut issues = Vec::new();
    let mut schemas = Vec::new();
    let mut catalog = serde_json::Map::new();
    for (index, (source, names)) in run.compiled().sources().iter().zip(headers).enumerate() {
        let odm = run.compiled().source_interpretation(index) == Some(SourceInterpretation::Odm);
        let exposed = if odm {
            let stored = names
                .iter()
                .map(|name| StoredField {
                    name,
                    kind: Some(ColumnType::Str),
                })
                .collect::<Vec<_>>();
            if let Err(findings) =
                odm_source::bind_fields(&stored).expect("bounded authored metadata")
            {
                issues.extend(
                    specification_diagnostics::findings(&Error::OdmFields(findings), Some(source))
                        .expect("registered ODM metadata findings"),
                );
                continue;
            }
            odm_source::FIELDS
                .iter()
                .map(|name| String::from(*name))
                .collect::<Vec<_>>()
        } else {
            names.clone()
        };
        catalog.insert(
            source.name.clone(),
            json!(exposed.iter().map(|name| (name, "str")).collect::<Vec<_>>()),
        );
        schemas.push(
            TableSchema::new(
                exposed
                    .into_iter()
                    .map(|name| Column {
                        name,
                        kind: ColumnType::Str,
                    })
                    .collect(),
            )
            .expect("authored schema"),
        );
    }
    if !issues.is_empty() {
        return json!({"status":"metadata_refusal", "issues":issues, "fields":odm_source::FIELDS, "catalog":catalog});
    }
    let references = schemas.iter().collect::<Vec<_>>();
    let binding = match run.compiled().bind_sources(&references) {
        Ok(_) => json!({"status":"bound", "issues":[]}),
        Err(error) => {
            json!({"status":"binding_refusal", "issues":specification_diagnostics::findings(&Error::Bind(error),Some(run.source()))})
        }
    };
    json!({"status":"prepared", "fields":odm_source::FIELDS, "catalog":catalog, "binding":binding})
}
