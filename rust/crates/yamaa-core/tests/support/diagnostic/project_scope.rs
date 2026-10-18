//! Project operation findings: names, unlinked producers, blocked consumers
//! and run-scope cycles, each with its independently authored mapping.
use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode, Diagnostic},
    json::Value,
    project_scope,
};

fn texts(values: &[&str]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|value| Value::String((*value).into()))
            .collect(),
    )
}

fn check(
    finding: &Diagnostic,
    condition: &str,
    requirement: &str,
    paths: &[&str],
    context: &[(&str, Value)],
) -> ConditionCode {
    assert_eq!(
        (
            finding.definition().phase,
            finding.definition().condition,
            finding.definition().requirement
        ),
        ("validation", condition, Some(requirement))
    );
    assert_eq!(finding.spec_paths, paths);
    assert_eq!(
        finding.context,
        context
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    );
    assert_eq!(finding.source_span, None);
    assert_eq!(finding.operand_route, None);
    finding.code
}

pub fn reached() -> BTreeSet<ConditionCode> {
    let mut reached = BTreeSet::new();
    reached.insert(check(
        &project_scope::duplicate_name(
            "DM",
            &[
                ("sdtm.specs[0]".into(), "/study/sdtm/dm.yaml".into()),
                ("adam.specs[1]".into(), "/study/adam/dm.yaml".into()),
            ],
        ),
        "duplicate_specification_name",
        "REQ-1338",
        &["sdtm.specs[0]", "adam.specs[1]"],
        &[
            ("name", Value::String("DM".into())),
            (
                "specifications",
                texts(&["/study/sdtm/dm.yaml", "/study/adam/dm.yaml"]),
            ),
        ],
    ));
    reached.insert(check(
        &project_scope::unknown_name("ADXX", &["DM", "ADSL", "AE"]),
        "unknown_specification_name",
        "REQ-1339",
        &[],
        &[
            ("name", Value::String("ADXX".into())),
            ("available", texts(&["ADSL", "AE", "DM"])),
        ],
    ));
    reached.insert(check(
        &project_scope::unlinked_producer("DM", "../sdtm/dm.csv", "/study/sdtm/dm.yaml"),
        "unlinked_producer",
        "REQ-1344",
        &["input.DM.path"],
        &[
            ("dataset", Value::String("DM".into())),
            ("path", Value::String("../sdtm/dm.csv".into())),
            ("producer", Value::String("/study/sdtm/dm.yaml".into())),
        ],
    ));
    reached.insert(check(
        &project_scope::producer_not_completed("DM", "/study/sdtm/dm.yaml"),
        "producer_not_completed",
        "REQ-1345",
        &["input.DM.schema"],
        &[
            ("dataset", Value::String("DM".into())),
            ("producer", Value::String("/study/sdtm/dm.yaml".into())),
        ],
    ));
    // A run-scope cycle names each specification on it.
    let cycle = project_scope::workflow_cycle("B", &["/a.yaml", "/b.yaml", "/a.yaml"]);
    reached.insert(check(
        &cycle,
        "producer_workflow_cycle",
        "REQ-1342",
        &["input.B.schema"],
        &[("cycle", texts(&["/a.yaml", "/b.yaml", "/a.yaml"]))],
    ));
    reached
}
