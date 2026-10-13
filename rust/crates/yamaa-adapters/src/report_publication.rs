//! All publication report storage and its largest wire prefix are admitted
//! before a publisher is entered. Completion only moves owned fields.
use crate::{report_projection::Budget, report_storage::StoredReport, specification_report::Error};
use serde_json::{json, Value};

const CODES: [&str; 10] = [
    "publication_incomplete",
    "producer_publication_incomplete",
    "producer_publication_nodes",
    "producer_publication_paths",
    "producer_publication_bytes",
    "producer_publication_work",
    "publication_report_limit",
    "target_admission",
    "publication_unwind",
    "target_preflight",
];
pub(crate) struct Location<'a> {
    pub(crate) key: usize,
    pub(crate) node: usize,
    pub(crate) field: Option<&'a str>,
    pub(crate) path: Option<&'a str>,
}
pub(crate) struct Prepared {
    errors: Vec<(Option<usize>, &'static str, Value)>,
    success: Value,
    failure: Value,
}
impl Prepared {
    pub(crate) fn new(
        report: &mut StoredReport,
        locations: &[Location<'_>],
        logged: bool,
        maximum: usize,
        budget: &mut Budget,
    ) -> Result<Self, Error> {
        let count = locations
            .len()
            .checked_add(1)
            .and_then(|n| n.checked_mul(CODES.len() + 1))
            .ok_or(Error::OutputLimit)?;
        budget.entries(count.checked_mul(16).ok_or(Error::OutputLimit)?)?;
        budget.text("success")?;
        budget.text("failure")?;
        let mut errors = Vec::with_capacity(count);
        let mut largest = 0;
        let mut largest_length = 0;
        for code in CODES.into_iter().chain(std::iter::once(
            yamaa_core::diagnostic::ConditionCode::OutputPublicationFailed
                .definition()
                .condition,
        )) {
            for location in std::iter::once(None).chain(locations.iter().map(Some)) {
                budget.text(code)?;
                if let Some(location) = location {
                    for text in location.field.into_iter().chain(location.path) {
                        budget.text(text)?;
                    }
                }
                let mut value =
                    json!({"stage":"publication","code":code,"node":location.map(|l|l.node)});
                if logged {
                    value["field"] = json!(location.and_then(|l| l.field));
                    value["path"] = json!(location.and_then(|l| l.path));
                }
                let length = serde_json::to_vec(&value)
                    .map_err(|_| Error::InvalidObservation)?
                    .len();
                if length > largest_length {
                    largest_length = length;
                    largest = errors.len();
                }
                errors.push((location.map(|l| l.key), code, value));
            }
        }
        // Complete artifact/entry arrays dominate every shorter prefix. False
        // is one encoded byte longer than true. The longest owned error covers
        // every admitted code/location, including failures before first entry.
        let worst = &errors[largest].2;
        budget.json(worst)?;
        report["publication"]["error"] = worst.clone();
        for entry in report["publication"]["entered"]
            .as_array_mut()
            .ok_or(Error::InvalidObservation)?
        {
            entry["published"] = Value::Bool(false);
        }
        crate::report_projection::encoded_within_limit(
            report,
            maximum.min(yamaa_engine::capacity::REPORT_ENCODED_BYTES),
        )?;
        Ok(Self {
            errors,
            success: json!("success"),
            failure: json!("failure"),
        })
    }
    pub(crate) fn finish(
        &mut self,
        report: &mut StoredReport,
        saved: bool,
        code: &str,
        key: Option<usize>,
    ) {
        report["publication"]["status"] = if saved {
            self.success.take()
        } else {
            self.failure.take()
        };
        report["publication"]["error"] = if saved {
            Value::Null
        } else {
            self.errors
                .iter_mut()
                .find(|(at, condition, _)| *at == key && *condition == code)
                .expect("admitted publication outcome")
                .2
                .take()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn report() -> StoredReport {
        StoredReport::empty(json!({"tables":[],"artifacts":[{"content":"held"}],
            "publication":{"status":"failure","error":null,"entered":[{"node":0,"published":true}]}}))
    }
    #[test]
    fn every_prefix_is_admitted_before_effects_and_completion_moves_owned_error_text() {
        let publication_code = yamaa_core::diagnostic::ConditionCode::OutputPublicationFailed
            .definition()
            .condition;
        let locations = [Location {
            key: 0,
            node: 0,
            field: Some("verification_log"),
            path: Some("é\n\"out.csv"),
        }];
        let mut candidate = report();
        let mut budget = Budget::new(16_777_216);
        let _ = Prepared::new(&mut candidate, &locations, true, 16_777_216, &mut budget).unwrap();
        let largest = candidate.observations_json(false).len();
        for (maximum, accepted) in [(largest, true), (largest - 1, false)] {
            let mut candidate = report();
            let mut budget = Budget::new(16_777_216);
            let prepared = Prepared::new(&mut candidate, &locations, true, maximum, &mut budget);
            assert_eq!(prepared.is_ok(), accepted);
        }
        let mut candidate = report();
        let mut budget = Budget::new(16_777_216);
        let mut prepared =
            Prepared::new(&mut candidate, &locations, true, largest, &mut budget).unwrap();
        let original = prepared
            .errors
            .iter()
            .find(|(at, code, _)| *at == Some(0) && *code == publication_code)
            .unwrap()
            .2["path"]
            .as_str()
            .unwrap()
            .as_ptr();
        prepared.finish(&mut candidate, false, publication_code, Some(0));
        assert_eq!(
            candidate["publication"]["error"]["path"]
                .as_str()
                .unwrap()
                .as_ptr(),
            original
        );
        assert!(candidate.observations_json(false).len() <= largest);
        assert_eq!(
            candidate["publication"]["error"],
            json!({"stage":"publication","code":publication_code,"node":0,"field":"verification_log","path":"é\n\"out.csv"})
        );
    }
    #[test]
    fn every_owned_actual_code_and_location_finishes_without_allocation_or_refusal() {
        let locations = [
            Location {
                key: 3,
                node: 19,
                field: Some("verification_log"),
                path: Some("first.csv"),
            },
            Location {
                key: 7,
                node: 19,
                field: Some("normalization_log"),
                path: Some("last.csv"),
            },
        ];
        let codes = CODES.into_iter().chain(std::iter::once(
            yamaa_core::diagnostic::ConditionCode::OutputPublicationFailed
                .definition()
                .condition,
        ));
        for code in codes {
            for key in [None, Some(3), Some(7)] {
                for logged in [false, true] {
                    let mut candidate = report();
                    let mut budget = Budget::new(16_777_216);
                    let mut prepared =
                        Prepared::new(&mut candidate, &locations, logged, 16_777_216, &mut budget)
                            .unwrap();
                    let owned = &prepared
                        .errors
                        .iter()
                        .find(|(at, condition, _)| *at == key && *condition == code)
                        .unwrap()
                        .2;
                    let code_pointer = owned["code"].as_str().unwrap().as_ptr();
                    let field_pointer = owned.get("field").and_then(Value::as_str).map(str::as_ptr);
                    let path_pointer = owned.get("path").and_then(Value::as_str).map(str::as_ptr);
                    let expected = owned.clone();
                    let complete_prefix = candidate.observations_json(false).len();
                    prepared.finish(&mut candidate, false, code, key);
                    assert_eq!(candidate["publication"]["error"], expected);
                    assert!(candidate.observations_json(false).len() <= complete_prefix);
                    assert_eq!(candidate["publication"]["error"]["code"], code);
                    assert_eq!(
                        candidate["publication"]["error"]["code"]
                            .as_str()
                            .unwrap()
                            .as_ptr(),
                        code_pointer
                    );
                    assert_eq!(
                        candidate["publication"]["error"]["node"],
                        key.map_or(Value::Null, |_| json!(19))
                    );
                    assert_eq!(
                        candidate["publication"]["error"]
                            .get("field")
                            .and_then(Value::as_str)
                            .map(str::as_ptr),
                        field_pointer
                    );
                    assert_eq!(
                        candidate["publication"]["error"]
                            .get("path")
                            .and_then(Value::as_str)
                            .map(str::as_ptr),
                        path_pointer
                    );
                }
            }
        }
    }
}
