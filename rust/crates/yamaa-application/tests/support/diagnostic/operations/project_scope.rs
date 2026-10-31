//! A project's scope (`project_scope::Project`): the specifications its
//! sections list, read for the `host` the input names, `python` unless
//! written, from the study directory's `project` file, and what each
//! question about them reports.
//!
//! `read_project` reports what refuses the project file, and nothing when it
//! is read. `project_check` reports what a check of the request naming the
//! `specification` with the project file reports
//! (`project_application::check`), which a project that cannot be read
//! refuses as `read_project` does; `read_project` takes the same
//! `specification` unread, so that one case states that both report the
//! same rows. The others read the project first, which must succeed:
//! `find_specification` reports what refuses the listed `name` and
//! `duplicate_names` the names listed twice, each from the project read for
//! its names alone (`project_scope::Names`), as selecting a specification by
//! name reads it; `check_findings` a project
//! check's findings, `plan_refusal` what refuses the run of the `section`'s
//! scope (every section unless written), `refused_step` what refuses the
//! step of that plan that runs the listed `name`, and `blocked_consumer`
//! what the step running `name` reports when the producers that `failed`
//! names by their listed names have failed, every producer unless written.
//! A listing that cannot be read has no name, so two questions find it by
//! its index among every listing, `listed` (section run order, then listing
//! order): `listing_failure` reports the findings that fail it before its
//! specification is read, each naming that specification as its source and
//! entry, as a project check reports a member's findings
//! (`project_scope::identify`), and `listing_step` what refuses the step of
//! the whole project's plan that runs it, nothing when no step runs it. Each
//! takes the study a case stages ([`super::study`]).
use super::{host, path, section, study, File, Reported, STUDY};
use crate::support::diagnostic::{
    form::Outcome,
    replay::{closed, field, items, text},
};
use yamaa_application::project_scope::{self, Names, Project};
use yamaa_core::json::Value;

pub(super) const FILES: &[File] = &[fixture!("project_scope.json")];

pub(super) fn run(operation: &str, input: &Value) -> Option<Reported> {
    let members = match operation {
        "read_project" | "project_check" => &["specification"][..],
        "duplicate_names" | "check_findings" => &[][..],
        "find_specification" => &["name"][..],
        "blocked_consumer" => &["name", "failed"][..],
        "plan_refusal" => &["section"][..],
        "refused_step" => &["section", "name"][..],
        "listing_failure" | "listing_step" => &["listed"][..],
        _ => return None,
    };
    closed(input, &[&STUDY[..], &["project", "host"], members].concat());
    let study = study(input);
    let project = path(&study, text(field(input, "project")));
    if operation == "project_check" {
        let issues = yamaa_application::project_application::check(
            &path(&study, text(field(input, "specification"))),
            &project,
            host(input),
        );
        return Some(Reported {
            issues,
            study: Some(study),
            outcome: Outcome::default(),
        });
    }
    if let "find_specification" | "duplicate_names" = operation {
        let names = Names::read_located(&project, Some(host(input)), None)
            .unwrap_or_else(|(issues, _)| panic!("the project is refused: {issues:?}"));
        let issues = match operation {
            "find_specification" => names.find(text(field(input, "name"))).err(),
            _ => Some(names.duplicates()),
        };
        return Some(Reported {
            issues: issues.unwrap_or_default(),
            study: Some(study),
            outcome: Outcome::default(),
        });
    }
    let read = Project::read(&project, Some(host(input)), None);
    let issues = match (operation, read) {
        ("read_project", Ok(_)) => vec![],
        ("read_project", Err(issues)) => issues,
        (_, Err(issues)) => panic!("the project is refused: {issues:?}"),
        (_, Ok(project)) => {
            let section = input.get("section").map(section);
            let name = || text(field(input, "name"));
            let listed = || {
                let listed = field(input, "listed");
                listed
                    .as_u64()
                    .and_then(|index| usize::try_from(index).ok())
                    .unwrap_or_else(|| panic!("listed is a listing's index, not {listed:?}"))
            };
            let step = |plan: &project_scope::Plan, name: &str| {
                plan.steps()
                    .iter()
                    .position(|step| project.listed()[step.listed()].name() == Some(name))
                    .unwrap_or_else(|| panic!("no step runs {name}"))
            };
            match operation {
                "check_findings" => project.check_findings(),
                "plan_refusal" => project.plan(section).refusal().to_vec(),
                "refused_step" => {
                    let plan = project.plan(section);
                    plan.steps()[step(&plan, name())].refused().to_vec()
                }
                "listing_failure" => {
                    let listed = &project.listed()[listed()];
                    let mut issues = listed.failure().to_vec();
                    project_scope::identify(&mut issues, listed.identity());
                    issues
                }
                "listing_step" => {
                    let index = listed();
                    project
                        .plan(None)
                        .steps()
                        .iter()
                        .find(|step| step.listed() == index)
                        .map(|step| step.refused().to_vec())
                        .unwrap_or_default()
                }
                _ => {
                    let failed = input.get("failed").map(|names| {
                        items(names)
                            .iter()
                            .map(|name| Some(text(name)))
                            .collect::<Vec<_>>()
                    });
                    let plan = project.plan(None);
                    plan.steps()[step(&plan, name())].blocked(&project, |producer| match &failed {
                        Some(failed) => failed.contains(&project.listed()[producer].name()),
                        None => true,
                    })
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
