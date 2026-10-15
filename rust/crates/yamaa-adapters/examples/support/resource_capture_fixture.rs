//! No path policy or capture cache is duplicated by this observation transport.
use serde_json::{json, Value};
use std::sync::Arc;
use yamaa_adapters::{file_resources::Resources, issue_rows::Issue};
use yamaa_core::specification_run::SourcePort;

#[derive(Default)]
pub struct Owner {
    resources: Option<Resources>,
    previous: Option<Arc<[u8]>>,
}
impl Owner {
    pub fn observe(&mut self, request: Value) -> Value {
        let operation = request["operation"].as_str().expect("authored operation");
        if operation == "open" {
            assert!(self.resources.is_none(), "one original resource owner");
            self.resources = Some(
                Resources::for_entry(request["base"].as_str().expect("declaring directory"))
                    .expect("authored declaring directory"),
            );
            return json!({"capture_reads":0});
        }
        let written = request["written"]
            .as_str()
            .expect("authored written spelling");
        let resources = self.resources.as_mut().expect("original owner");
        let result = if operation == "resolve" {
            resources
                .resolve(written)
                .map(|identity| json!({"identity":identity}))
        } else {
            assert_eq!(operation, "capture");
            resources.capture(written, 8_388_608).map(|(bytes, created)| {
                let same_snapshot = self.previous.as_ref().is_some_and(|prior| Arc::ptr_eq(prior, &bytes));
                self.previous = Some(Arc::clone(&bytes));
                json!({"bytes":bytes.as_ref(), "created":created, "same_snapshot":same_snapshot})
            })
        };
        match result {
            Ok(mut observation) => {
                observation["capture_reads"] = json!(resources.capture_reads());
                observation
            }
            Err(error) => {
                let failure = resources
                    .resource_failure(&error)
                    .expect("actual classified resource refusal");
                let issue = Issue::from_core(failure.field_diagnostic("input.DM.path", written))
                    .expect("portable original resource diagnostic");
                json!({"issue":issue, "capture_reads":resources.capture_reads()})
            }
        }
    }
}
