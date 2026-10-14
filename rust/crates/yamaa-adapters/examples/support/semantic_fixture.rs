//! Observe the existing compiler/binder over independently authored metadata.
//! No cells, resolver callbacks, evaluator, or publication capability are created.
use serde_json::{json, Value};
use yamaa_adapters::{issue_rows::Issue, yaml_decode::decode_yaml};
use yamaa_core::{
    diagnostic::Diagnostic,
    schema::{SpecificationDocument, ValidationBudget},
    specification::{BindError, PrepareError, PreparedSpecification},
    table::{Column, TableSchema},
    value::ColumnType,
};

fn row(diagnostic: Diagnostic) -> Value {
    let span = diagnostic.source_span.map(|span| [span.start, span.end]);
    let mut row = Issue::from_core(diagnostic)
        .expect("registered core semantic diagnostic")
        .as_value();
    row["context"] =
        serde_json::from_str(row["context"].as_str().unwrap()).expect("canonical issue context");
    row["span"] = json!(span);
    row
}

pub fn observe(input: Value) -> Value {
    if let Some(mode) = input["mode"].as_str() {
        use yamaa_core::{aggregate_compiler, numeric_compiler};
        let expression = input["expression"].as_str().expect("authored expression");
        let path = input["path"].as_str().expect("authored expression path");
        let kind = match mode {
            "numeric-type" => Some(
                numeric_compiler::compile_numeric_with_policy(
                    expression,
                    path,
                    Default::default(),
                    numeric_compiler::MathPolicy::PortableLibmV1,
                )
                .expect("authored admitted numeric type witness")
                .result_type(),
            ),
            "aggregate-type" => {
                aggregate_compiler::compile_aggregate(expression, path, Default::default())
                    .expect("authored admitted aggregate type witness")
                    .numeric_result_type()
            }
            other => panic!("unsupported semantic fixture mode: {other}"),
        };
        return json!({"result_type":kind.map(|kind| match kind {
            ColumnType::Str => "str", ColumnType::Int => "int",
            ColumnType::Float => "float", ColumnType::Date => "date",
            ColumnType::DateTime => "datetime",
        })});
    }
    let document = decode_yaml(
        input["specification"].to_string().as_bytes(),
        Default::default(),
    )
    .expect("authored semantic fixture decodes");
    let document = SpecificationDocument::admit(
        document.document,
        &mut ValidationBudget::new(Default::default()),
    )
    .expect("bounded model admission")
    .expect("complete normalized semantic fixture");
    let driver = document.default_driver().map(str::to_owned);
    let prepared = match PreparedSpecification::prepare(&document) {
        Ok(prepared) => prepared,
        Err(PrepareError::Invalid(findings)) => {
            return json!({"issues": findings.into_iter()
                .map(|finding| row(finding.diagnostic())).collect::<Vec<_>>(),
                "driver": driver});
        }
        Err(error) => panic!("semantic fixture compiler policy refused: {error:?}"),
    };
    let mut issues = prepared
        .check_declaration_diagnostics()
        .into_iter()
        .map(row)
        .collect::<Vec<_>>();
    if input["bind"].as_bool() == Some(false) {
        return json!({"issues":issues, "driver":driver});
    }
    let sources = prepared
        .sources()
        .iter()
        .map(|source| {
            let fields = input["sources"][&source.name]
                .as_object()
                .expect("explicit authored source catalog for every source");
            TableSchema::new(
                fields
                    .iter()
                    .map(|(name, kind)| Column {
                        name: name.clone(),
                        kind: match kind.as_str().expect("authored closed type name") {
                            "str" => ColumnType::Str,
                            "int" => ColumnType::Int,
                            "float" => ColumnType::Float,
                            "date" => ColumnType::Date,
                            "datetime" => ColumnType::DateTime,
                            other => panic!("unsupported fixture type: {other}"),
                        },
                    })
                    .collect(),
            )
            .expect("authored source fields are unique")
        })
        .collect::<Vec<_>>();
    match prepared.bind_sources(&sources.iter().collect::<Vec<_>>()) {
        Ok(_) => (),
        Err(BindError::Invalid(findings)) => {
            for finding in findings {
                for diagnostic in finding
                    .diagnostics(prepared.source())
                    .expect("registered core binding diagnostic")
                {
                    let item = row(diagnostic);
                    if !issues.contains(&item) {
                        issues.push(item);
                    }
                }
            }
        }
        Err(error) => panic!("semantic fixture binder policy refused: {error:?}"),
    }
    json!({"issues": issues, "driver": driver})
}
