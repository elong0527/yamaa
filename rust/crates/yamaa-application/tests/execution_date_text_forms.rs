#![cfg(feature = "yaml")]
//! Dates read from their declared forms at run time (TIME-22), against the
//! authored table of `fixtures/date_text_forms.json`: `date_text` reads each
//! declared form into the collected date text that `impute` completes, a
//! slashed date in the order of day and month its list names, and refuses
//! text in no declared form unless `invalid` answers it (COLUMN-10,
//! COLUMN-14).
use crate::support::study::{issues, Study};
use yamaa_adapters::json;
use yamaa_core::json::Value;

fn fixture() -> Value {
    yamaa_adapters::json_wire::decode(include_str!("fixtures/date_text_forms.json")).unwrap()
}

/// `RAW.TEXT` read by `date_text` over `forms` into `DTC`, which `impute`
/// completes into the date `DT`, a month to its first day; `handler` is
/// written after the forms.
fn specification(forms: &Value, handler: &str) -> String {
    let forms = forms
        .as_array()
        .unwrap()
        .iter()
        .map(|form| form.as_str().unwrap())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "yamaa: \"1.0\"\n\
         type: spec\n\
         domain: XX\n\
         keys: [ID]\n\
         input:\n\
         \x20 RAW: {{path: raw.csv, types: {{ID: int}}}}\n\
         output: {{path: out.csv, columns: [ID, DTC, DT]}}\n\
         columns:\n\
         \x20 - {{name: ID, type: int, derivation: RAW.ID}}\n\
         \x20 - name: DTC\n\
         \x20   type: str\n\
         \x20   derivation:\n\
         \x20     date_text: {{source: RAW.TEXT, forms: [{forms}]{handler}}}\n\
         \x20 - name: DT\n\
         \x20   type: date\n\
         \x20   derivation:\n\
         \x20     impute: {{type: date, source: DTC, day: first}}\n"
    )
}

/// `raw.csv`: one record for each text, its `ID` counting from 1, and a
/// missing text written as an empty field.
fn input<'a>(texts: impl IntoIterator<Item = Option<&'a str>>) -> String {
    let mut csv = String::from("ID,TEXT\n");
    for (index, text) in texts.into_iter().enumerate() {
        csv.push_str(&format!("{},{}\n", index + 1, text.unwrap_or_default()));
    }
    csv
}

/// Each declared form reads into the collected date text it denotes, a
/// slashed date in the order its list declares, and `impute` completes a
/// month to its first day; a missing text gives missing, counted only where
/// `impute` reads it (TIME-22, TIME-8).
///
/// `adam-adcm-free-text-start-date` pins the month-first readings, the month
/// completed to its first day and the missing start. No benchmark pins the
/// day-first order of the `dmy_slash` list or the `impute.missing` count.
#[test]
fn each_declared_form_reads_into_collected_text_that_impute_completes() {
    let fixture = fixture();
    let readings = fixture["readings"].as_array().unwrap();
    assert_eq!(readings.len(), 7);
    let missing = readings
        .iter()
        .filter(|reading| reading["text"].is_null())
        .count();
    for order in ["mdy_slash", "dmy_slash"] {
        let study = Study::new("date-text-forms");
        study.write("spec.yaml", specification(&fixture["forms"][order], ""));
        study.write(
            "raw.csv",
            input(readings.iter().map(|reading| reading["text"].as_str())),
        );
        let mut domain = study
            .run("spec.yaml")
            .unwrap_or_else(|issues| panic!("{order}: {issues:?}"));
        assert!(domain.issues().is_empty(), "{order}: {:?}", domain.issues());
        assert!(domain.save().unwrap(), "{order}");
        let mut expected = String::from("ID,DTC,DT\n");
        for (index, reading) in readings.iter().enumerate() {
            let cell = |key: &str| reading[order][key].as_str().unwrap_or_default();
            expected.push_str(&format!(
                "{},{},{}\n",
                index + 1,
                cell("date_text"),
                cell("impute")
            ));
        }
        assert_eq!(
            String::from_utf8(study.saved("out.csv")).unwrap(),
            expected,
            "{order}"
        );
        assert_eq!(
            domain.observations()["handler_counts"],
            json!([{"spec_path": "columns.DT.derivation.impute.missing", "handler": "missing", "count": missing}]),
            "{order}"
        );
    }
}

/// Text in no declared form, a day outside its month among it, fails at the
/// `impute` stage with its operation, source, value and record, whichever
/// slash order the list declares (TIME-22).
///
/// `negative-date-text-no-form` pins the refusal of text in no form, with a
/// list that names neither slash order. No benchmark pins the out-of-range
/// `29FEB2001`, or either refusal under a list that names a slash order.
#[test]
fn text_in_no_declared_form_fails_with_its_source_value_and_record() {
    let fixture = fixture();
    let refusals = fixture["refusals"].as_array().unwrap();
    assert_eq!(refusals.len(), 2);
    for order in ["mdy_slash", "dmy_slash"] {
        for refusal in refusals {
            let label = format!("{order}: {}", refusal["case"].as_str().unwrap());
            let study = Study::new("date-text-forms");
            study.write("spec.yaml", specification(&fixture["forms"][order], ""));
            study.write("raw.csv", input([refusal["text"].as_str()]));
            assert_eq!(
                issues(study.run("spec.yaml")),
                [refusal["issue"].clone()],
                "{label}"
            );
        }
    }
}

/// A declared `invalid` answers text in no declared form with its literal,
/// counted for each record it answers (TIME-22, COLUMN-14).
///
/// No benchmark declares `invalid` on a `date_text`, so none pins its answer
/// or its count.
#[test]
fn a_declared_invalid_answers_text_in_no_declared_form() {
    let fixture = fixture();
    let refusals = fixture["refusals"].as_array().unwrap();
    let study = Study::new("date-text-forms");
    study.write(
        "spec.yaml",
        specification(&fixture["forms"]["mdy_slash"], ", invalid: null"),
    );
    study.write(
        "raw.csv",
        input(refusals.iter().map(|refusal| refusal["text"].as_str())),
    );
    let mut domain = study.run("spec.yaml").unwrap();
    assert!(domain.issues().is_empty(), "{:?}", domain.issues());
    assert!(domain.save().unwrap());
    assert_eq!(
        String::from_utf8(study.saved("out.csv")).unwrap(),
        "ID,DTC,DT\n1,,\n2,,\n"
    );
    assert_eq!(
        domain.observations()["handler_counts"],
        json!([
            {"spec_path": "columns.DTC.derivation.date_text.invalid", "handler": "invalid", "count": 2},
            {"spec_path": "columns.DT.derivation.impute.missing", "handler": "missing", "count": 2},
        ])
    );
}
