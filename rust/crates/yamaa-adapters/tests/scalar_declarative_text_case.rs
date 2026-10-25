#![cfg(feature = "yaml")]
//! Text case: `str_case`'s `proper` mode through a specification (TEXT-5).
//! The test reads the shared proper-case rows of
//! `fixtures/text_case_reference.json`, which core's `scalar_sqrt.rs` reads
//! through `yamaa_core::text_case` too, so both crates prove the same
//! authored rows, and lays them out by code, one column for each delimiter
//! list a row declares. The case form reads no shared row table, so this
//! stays a function; the family's other cases are data in
//! `tests/fixtures/diagnostics/text_case.json` (#3798), and the API
//! sidecar `fixtures/migrated_api_contracts.json` holds what their refused
//! runs return.
/// `list` as a YAML double-quoted scalar. A list a row declares is
/// printable ASCII or the tab, which the scalar writes as its escape.
fn yaml_quoted(list: &str) -> String {
    let mut quoted = String::from('"');
    for scalar in list.chars() {
        match scalar {
            '"' | '\\' => {
                quoted.push('\\');
                quoted.push(scalar);
            }
            '\t' => quoted.push_str("\\t"),
            ' '..='~' => quoted.push(scalar),
            other => panic!("a declared list holds {other:?}"),
        }
    }
    quoted.push('"');
    quoted
}
/// Already data: the shared proper-case rows of
/// `fixtures/text_case_reference.json`, a row table the case form does not
/// read.
#[test]
fn proper_raises_the_first_scalar_and_each_scalar_after_a_delimiter() {
    // The shared proper-case rows (TEXT-5), each read in the column
    // of its list: P0 takes SAS PROPCASE's six delimiters, and each
    // later column one list that a row declares.
    let reference: yamaa_core::json::Value =
        yamaa_adapters::json_wire::decode(include_str!("fixtures/text_case_reference.json"))
            .unwrap();
    let rows = reference["proper"].as_array().unwrap();
    assert_eq!(rows.len(), 23, "the authored proper-case rows");
    let list = |row: &yamaa_core::json::Value| {
        row.get("delimiters")
            .map(|list| list.as_str().unwrap().to_owned())
    };
    let mut lists = vec![None];
    for row in rows {
        if !lists.contains(&list(row)) {
            lists.push(list(row));
        }
    }
    let names = (0..lists.len())
        .map(|index| format!("P{index}"))
        .collect::<Vec<_>>();
    let mut source = format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: input.csv}}\noutput: {{path: output.csv, columns: [ID, {}]}}\ncolumns:\n  - {{name: ID, type: str, derivation: SRC.ID}}\n",
        names.join(", ")
    );
    for (name, list) in names.iter().zip(&lists) {
        let delimiters = list
            .as_deref()
            .map(|list| format!(", delimiters: {}", yaml_quoted(list)))
            .unwrap_or_default();
        source.push_str(&format!(
            "  - {{name: {name}, type: str, derivation: {{str_case: {{source: SRC.TEXT, to: proper{delimiters}}}}}}}\n"
        ));
    }
    let mut input = String::from("ID,TEXT\n");
    for (index, row) in rows.iter().enumerate() {
        let text = row["source"].as_str().unwrap().replace('"', "\"\"");
        input.push_str(&format!("{index:02},\"{text}\"\n"));
    }
    let study = crate::support::study::Study::new("str-case");
    study.write("spec.yaml", &source);
    study.write("input.csv", &input);
    let result = study
        .run("spec.yaml")
        .unwrap_or_else(|issues| panic!("preparation {issues:?}"));
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    let table = result.output().unwrap();
    let text = |name: &str| {
        let column = table.names().position(|written| written == name).unwrap();
        table
            .batches()
            .iter()
            .flat_map(|batch| {
                batch
                    .column(column)
                    .as_any()
                    .downcast_ref::<arrow_array::StringArray>()
                    .unwrap()
                    .iter()
                    .map(|value| value.map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    let ids = text("ID");
    let columns = names.iter().map(|name| text(name)).collect::<Vec<_>>();
    for (index, row) in rows.iter().enumerate() {
        let label = format!("{:?} with {:?}", row["source"], list(row));
        let column = lists
            .iter()
            .position(|written| *written == list(row))
            .unwrap();
        assert_eq!(
            (ids[index].as_deref(), columns[column][index].as_deref()),
            (
                Some(format!("{index:02}").as_str()),
                row["expected"].as_str()
            ),
            "{label}"
        );
    }
}
