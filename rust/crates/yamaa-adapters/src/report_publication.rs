//! Publication owns its outcome fields before entering a publisher.
//! Completion preserves the actual prefix and moves the owned error details.
use crate::json;
use crate::{report_storage::StoredReport, specification_report::Error};
use yamaa_core::json::Value;

const CODES: [&str; 6] = [
    "publication_incomplete",
    "producer_publication_incomplete",
    "producer_build_failed",
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
    ) -> Result<Self, Error> {
        let count = (locations.len() + 1) * (CODES.len() + 1);
        let mut errors = Vec::with_capacity(count);
        for code in CODES.into_iter().chain(std::iter::once(
            yamaa_core::diagnostic::ConditionCode::OutputPublicationFailed
                .definition()
                .condition,
        )) {
            for location in std::iter::once(None).chain(locations.iter().map(Some)) {
                let mut value =
                    json!({"stage":"publication","code":code,"node":location.map(|l|l.node)});
                if logged {
                    value["field"] = json!(location.and_then(|l| l.field));
                    value["path"] = json!(location.and_then(|l| l.path));
                }
                errors.push((location.map(|l| l.key), code, value));
            }
        }
        for entry in report["publication"]["entered"]
            .as_array_mut()
            .ok_or(Error::InvalidObservation)?
        {
            entry["published"] = Value::Bool(false);
        }
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
            field: Some("report"),
            path: Some("é\n\"out.csv"),
        }];
        let mut candidate = report();
        let mut prepared = Prepared::new(&mut candidate, &locations, true).unwrap();
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
        assert_eq!(
            candidate["publication"]["error"],
            json!({"stage":"publication","code":publication_code,"node":0,"field":"report","path":"é\n\"out.csv"})
        );
    }
    #[test]
    fn every_owned_actual_code_and_location_finishes_without_allocation_or_refusal() {
        let locations = [
            Location {
                key: 3,
                node: 19,
                field: Some("report"),
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
                    let mut prepared = Prepared::new(&mut candidate, &locations, logged).unwrap();
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
                    prepared.finish(&mut candidate, false, code, key);
                    assert_eq!(candidate["publication"]["error"], expected);
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
