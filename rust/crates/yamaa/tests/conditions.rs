//! `yamaa::conditions` gives the condition catalogue (#3700): every condition
//! the registry holds and every condition a release retired, in name order,
//! with the seven columns of the catalogue.
//!
//! The `conditions` family of the shared truth, which the Python and R
//! suites and the command line read too, names every row and authors one row
//! of each shape, written by hand from `yaml/conditions.yaml` and the core
//! registry's causes: rules in card order across prefixes and numbers, a
//! condition whose issues cite no rule, the four renames and the seven
//! retirements of 0.6.0.

use std::sync::LazyLock;

use yamaa::{Condition, CONDITION_COLUMNS};
#[cfg(feature = "arrow")]
use yamaa::{DataType, Field};
use yamaa_core::json::Value;

/// The shared truth's `conditions` family, read once for every test here.
static TRUTH: LazyLock<Value> = LazyLock::new(|| {
    let truth =
        yamaa_adapters::json_wire::decode(include_str!("../../../tests/host-fixtures/cases.json"))
            .expect("the shared truth is JSON");
    truth["families"]["conditions"].clone()
});

fn texts(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("a list")
        .iter()
        .map(|item| item.as_str().expect("text"))
        .collect()
}

/// One row as its seven cells, in column order.
type Cells<'a> = (
    &'a str,
    &'a str,
    Vec<&'a str>,
    Vec<&'a str>,
    Option<&'a str>,
    Option<&'a str>,
    Option<&'a str>,
);

fn cells(entry: &Condition) -> Cells<'_> {
    (
        entry.condition.as_str(),
        entry.severity.as_str(),
        entry.phases.iter().map(|phase| phase.as_str()).collect(),
        entry.rules.clone(),
        entry.message,
        entry.renamed_from,
        entry.retired_in,
    )
}

#[test]
fn the_catalogue_names_every_registered_and_retired_condition_in_name_order() {
    let catalogue = yamaa::conditions();
    let names = catalogue
        .entries()
        .iter()
        .map(|entry| entry.condition.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, texts(&TRUTH["names"]));
    assert_eq!(texts(&TRUTH["columns"]), CONDITION_COLUMNS);
}

#[test]
fn each_authored_row_is_the_entry_of_its_condition() {
    let catalogue = yamaa::conditions();
    for row in TRUTH["rows"].as_array().unwrap() {
        let row = row.as_array().unwrap();
        let expected: Cells<'_> = (
            row[0].as_str().unwrap(),
            row[1].as_str().unwrap(),
            texts(&row[2]),
            texts(&row[3]),
            row[4].as_str(),
            row[5].as_str(),
            row[6].as_str(),
        );
        let entry = catalogue
            .entries()
            .iter()
            .find(|entry| entry.condition.as_str() == expected.0)
            .unwrap_or_else(|| panic!("no entry for {}", expected.0));
        assert_eq!(cells(entry), expected, "{}", expected.0);
    }
}

/// A retired condition keeps its row with the release that retired it and
/// nothing a registration gives; every other condition has phases and a
/// message, as `yaml/conditions.yaml` requires of a registration.
#[test]
fn a_retired_condition_has_no_phase_rule_or_message() {
    for entry in yamaa::conditions().entries() {
        let retired = entry.retired_in.is_some();
        assert_eq!(
            (
                entry.phases.is_empty(),
                entry.message.is_none(),
                retired && !entry.rules.is_empty(),
                retired && entry.renamed_from.is_some(),
            ),
            (retired, retired, false, false),
            "{}",
            entry.condition
        );
    }
}

/// A former name is no condition of the catalogue, so a program that
/// matched it finds the one condition that took it.
#[test]
fn a_former_name_names_no_condition_of_the_catalogue() {
    let entries = yamaa::conditions().entries().to_vec();
    for entry in &entries {
        if let Some(former) = entry.renamed_from {
            let claims = entries
                .iter()
                .filter(|other| {
                    other.condition.as_str() == former || other.renamed_from == Some(former)
                })
                .map(|other| other.condition)
                .collect::<Vec<_>>();
            assert_eq!(claims, [entry.condition], "{former}");
        }
    }
}

/// The rules are distinct card IDs in card order: by prefix, then by number.
#[test]
fn the_rules_are_distinct_card_ids_in_card_order() {
    let order = |rule: &str| {
        let (prefix, number) = rule.rsplit_once('-').expect("a card ID");
        (
            prefix.to_owned(),
            number.parse::<u32>().expect("a card number"),
        )
    };
    for entry in yamaa::conditions().entries() {
        let keys = entry
            .rules
            .iter()
            .map(|rule| order(rule))
            .collect::<Vec<_>>();
        assert!(
            keys.windows(2).all(|pair| pair[0] < pair[1]),
            "{}: {:?}",
            entry.condition,
            entry.rules
        );
    }
}

#[cfg(feature = "arrow")]
#[test]
fn the_table_has_the_catalogue_columns_and_one_row_per_condition() {
    let catalogue = yamaa::conditions();
    let table = catalogue.table();
    let list = DataType::List(Field::new("item", DataType::Utf8, true).into());
    let columns = table
        .columns()
        .iter()
        .map(|field| (field.name().as_str(), field.data_type().clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        columns,
        [
            ("condition", DataType::Utf8),
            ("severity", DataType::Utf8),
            ("phases", list.clone()),
            ("rules", list),
            ("message", DataType::Utf8),
            ("renamed_from", DataType::Utf8),
            ("retired_in", DataType::Utf8),
        ]
    );
    assert_eq!(table.rows(), catalogue.entries().len());
}

/// The command line's forms of an entry: its line, the cells separated by
/// tabs with a list joined by `, ` and a missing cell empty, and its object,
/// with its lists as lists and a missing cell null.
#[test]
fn an_entry_prints_as_a_tab_separated_line_and_one_object() {
    let catalogue = yamaa::conditions();
    let entry = |name: &str| {
        catalogue
            .entries()
            .iter()
            .find(|entry| entry.condition.as_str() == name)
            .unwrap()
    };
    for (name, line, object) in [
        (
            "standard_model_mismatch",
            "standard_model_mismatch\terror\tvalidation\tDEFINE-6, SUB-3\t\
             {value} is outside the {model} model of section {section}\t\
             unknown_standard_family\t",
            "{\"condition\":\"standard_model_mismatch\",\
             \"message\":\"{value} is outside the {model} model of section {section}\",\
             \"phases\":[\"validation\"],\"renamed_from\":\"unknown_standard_family\",\
             \"retired_in\":null,\"rules\":[\"DEFINE-6\",\"SUB-3\"],\"severity\":\"error\"}",
        ),
        (
            "month_required",
            "month_required\terror\t\t\t\t\t0.6.0",
            "{\"condition\":\"month_required\",\"message\":null,\"phases\":[],\
             \"renamed_from\":null,\"retired_in\":\"0.6.0\",\"rules\":[],\"severity\":\"error\"}",
        ),
    ] {
        let entry = entry(name);
        assert_eq!(
            (entry.text(), entry.to_value().to_json()),
            (line.to_owned(), object.to_owned()),
            "{name}"
        );
    }
    let lines = catalogue.lines();
    assert_eq!(
        (lines[0].as_str(), lines.len()),
        (
            "condition\tseverity\tphases\trules\tmessage\trenamed_from\tretired_in",
            catalogue.entries().len() + 1
        )
    );
}
