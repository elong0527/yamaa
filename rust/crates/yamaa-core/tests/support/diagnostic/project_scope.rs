//! Project operation findings: names, sections, unlinked producers, blocked
//! consumers, run-scope cycles, saves with nothing to write and stale
//! artifacts, each with its independently authored mapping.
use super::{rendered, Record};
use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ConditionCode, Diagnostic},
    json::Value,
    project_scope, run_report, submission_staleness,
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
    rule: &str,
    paths: &[&str],
    context: &[(&str, Value)],
) -> ConditionCode {
    check_at(finding, "validation", condition, rule, paths, context)
}

fn check_at(
    finding: &Diagnostic,
    phase: &str,
    condition: &str,
    rule: &str,
    paths: &[&str],
    context: &[(&str, Value)],
) -> ConditionCode {
    assert_eq!(
        (
            finding.definition().phase,
            finding.definition().condition,
            finding.definition().rule
        ),
        (phase, condition, Some(rule))
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
    rendered(&finding)
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
        "PROJECT-5",
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
        "PROJECT-6",
        &[],
        &[
            ("name", Value::String("ADXX".into())),
            ("available", texts(&["ADSL", "AE", "DM"])),
        ],
    ));
    // A section outside the three, or one the project does not declare,
    // names every declared section, sorted.
    reached.insert(check(
        &project_scope::unknown_section("send", &["sdtm", "adam"]),
        "unknown_section",
        "PROJECT-14",
        &[],
        &[
            ("section", Value::String("send".into())),
            ("sections", texts(&["adam", "sdtm"])),
        ],
    ));
    reached.insert(check(
        &project_scope::unlinked_producer("DM", "../sdtm/dm.csv", "/study/sdtm/dm.yaml"),
        "unlinked_producer",
        "PROJECT-10",
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
        "PROJECT-11",
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
        "PROJECT-8",
        &["input.B.schema"],
        &[("cycle", texts(&["/a.yaml", "/b.yaml", "/a.yaml"]))],
    ));
    // Saving a run or a submission that built nothing is refused at output,
    // each under its own card.
    reached.insert(check_at(
        &project_scope::run_nothing_to_save(),
        "output",
        "nothing_to_save",
        "PROJECT-12",
        &[],
        &[],
    ));
    reached.insert(check_at(
        &project_scope::submission_nothing_to_save(),
        "output",
        "nothing_to_save",
        "PROJECT-13",
        &[],
        &[],
    ));
    // A run report that records another artifact than the one a section
    // composes: reported at the listing's report, naming the dataset, the
    // report and what disagrees.
    let guard = submission_staleness::Guard {
        path: "sdtm.specs[0].output.report".into(),
        dataset: "DM",
        report: "dm-report.csv",
    };
    let recorded = run_report::Recorded {
        artifact: "dm.csv".into(),
        run: run_report::Run::Succeeded { rows: 2 },
        inputs: Vec::new(),
    };
    reached.insert(check(
        &guard.run(&recorded, "out/dm.csv").unwrap_err().record(),
        "stale_artifact",
        "PROJECT-15",
        &["sdtm.specs[0].output.report"],
        &[
            ("dataset", Value::String("DM".into())),
            ("report", Value::String("dm-report.csv".into())),
            ("reason", Value::String("artifact_path".into())),
            ("recorded", Value::String("dm.csv".into())),
            ("actual", Value::String("out/dm.csv".into())),
        ],
    ));
    reached
}
