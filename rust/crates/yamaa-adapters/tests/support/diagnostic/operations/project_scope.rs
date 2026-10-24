//! A project's scope (`project_scope::Project`): the specifications its
//! sections list, read for the Python host from the study directory's
//! `project` file, and what each question about them reports.
//!
//! `read_project` reports what refuses the project file, and nothing when it
//! is read. The others read it first, which must succeed:
//! `find_specification` reports what refuses the listed `name`,
//! `duplicate_names` the names listed twice, `check_findings` a project
//! check's findings, `plan_refusal` what refuses the run of the `section`'s
//! scope (every section unless written), `refused_step` what refuses the
//! step of that plan that runs the listed `name`, and `blocked_consumer`
//! what the step running `name` reports when every producer it reads has
//! failed. Each takes `files` and `directories` as the file route does.
use super::{path, study, File, Reported};
use crate::support::diagnostic::{
    form::Outcome,
    replay::{closed, field, text},
};
use yamaa_adapters::project_scope::Project;
use yamaa_core::{json::Value, project_environment::Submission, project_function::Language};

pub(super) const FILES: &[File] = &[fixture!("project_scope.json")];

pub(super) fn run(operation: &str, input: &Value) -> Option<Reported> {
    let members = match operation {
        "read_project" | "duplicate_names" | "check_findings" => &[][..],
        "find_specification" | "blocked_consumer" => &["name"][..],
        "plan_refusal" => &["section"][..],
        "refused_step" => &["section", "name"][..],
        _ => return None,
    };
    closed(
        input,
        &[&["files", "directories", "project"][..], members].concat(),
    );
    let study = study(input);
    let read = Project::read(
        &path(&study, text(field(input, "project"))),
        Some(Language::Python),
        None,
    );
    let issues = match (operation, read) {
        ("read_project", Ok(_)) => vec![],
        ("read_project", Err(issues)) => issues,
        (_, Err(issues)) => panic!("the project is refused: {issues:?}"),
        (_, Ok(project)) => {
            let section = input.get("section").map(|section| match text(section) {
                "sdtm" => Submission::Sdtm,
                "adam" => Submission::Adam,
                "send" => Submission::Send,
                other => panic!("no section {other:?}"),
            });
            let name = || text(field(input, "name"));
            let step = |plan: &yamaa_adapters::project_scope::Plan, name: &str| {
                plan.steps()
                    .iter()
                    .position(|step| project.listed()[step.listed()].name() == Some(name))
                    .unwrap_or_else(|| panic!("no step runs {name}"))
            };
            match operation {
                "find_specification" => project.find(name()).err().unwrap_or_default(),
                "duplicate_names" => project.duplicates(),
                "check_findings" => project.check_findings(),
                "plan_refusal" => project.plan(section).refusal().to_vec(),
                "refused_step" => {
                    let plan = project.plan(section);
                    plan.steps()[step(&plan, name())].refused().to_vec()
                }
                _ => {
                    let plan = project.plan(None);
                    plan.steps()[step(&plan, name())].blocked(&project, |_| true)
                }
            }
        }
    };
    Some(Reported {
        issues,
        study: Some(study),
        outcome: Outcome::default(),
    })
}
