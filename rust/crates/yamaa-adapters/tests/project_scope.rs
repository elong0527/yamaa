#![cfg(feature = "yaml")]
//! Project-scope contracts in the adapters: listed names, run-scope plans,
//! held producer artifacts, report-only saves and the preflight of one save
//! across results.
use crate::support::Edit as _;
use std::{fs, sync::Arc};
use yamaa_adapters::{
    file_application::{self, Domain},
    file_resources::HeldArtifacts,
    file_workflow,
    project_scope::Project,
    publication::{self, Cause},
    specification_report::Identity,
    specification_run::NoActivation,
};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::{
    json::Value, project_environment::Submission, project_function::Language,
    specification_run::SourceOrigin,
};

struct Study(crate::support::Scratch);
impl Study {
    fn new() -> Self {
        let root = crate::support::Scratch::canonical("project-scope");
        fs::create_dir_all(root.join("out")).unwrap();
        Self(root)
    }
    fn write(&self, name: &str, contents: impl AsRef<[u8]>) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    /// A file's path as the engine reports it, with forward slashes.
    fn path(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().replace('\\', "/")
    }
}

fn id() -> Identity<'static> {
    crate::support::study::identity("project-scope", "spec.yaml")
}

fn scope() -> yamaa_core::application_observer::Scope<'static> {
    yamaa_core::application_observer::Scope::new(&yamaa_core::application_observer::Noop)
}

fn spec(domain: &str, input: &str, output: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: spec\ndomain: {domain}\nkeys: [ID]\ninput: {{SRC: {input}}}\ncolumns:\n  - {{name: ID, type: int, label: Identifier, derivation: SRC.ID}}\noutput: {{path: {output}, columns: [ID], report: {output}.report.csv}}\n"
    )
}

const RAW: &str = "{path: ../raw.csv, types: {ID: int}}";

/// DM in SDTM; ADSL and ADAE in ADaM, both linked to DM, and ADAE to ADSL.
/// The project declares ADaM before SDTM.
fn study() -> Study {
    let study = Study::new();
    study.write("raw.csv", "ID\n1\n2\n");
    study.write("sdtm/dm.yaml", spec("DM", RAW, "../out/dm.csv"));
    study.write(
        "adam/adsl.yaml",
        spec(
            "ADSL",
            "{path: ../out/dm.csv, schema: ../sdtm/dm.yaml}",
            "../out/adsl.csv",
        ),
    );
    study.write(
        "adam/adae.yaml",
        spec(
            "ADAE",
            "{path: ../out/adsl.csv, schema: adsl.yaml}",
            "../out/adae.csv",
        ),
    );
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nadam:\n  specs: [adam/adae.yaml, adam/adsl.yaml]\nsdtm:\n  specs: [sdtm/dm.yaml]\n",
    );
    study
}

fn read(study: &Study) -> Project {
    Project::read(&study.path("project.yaml"), Some(Language::Python), None)
        .unwrap_or_else(|issues| panic!("{issues:?}"))
}

fn names(project: &Project, steps: impl IntoIterator<Item = usize>) -> Vec<String> {
    steps
        .into_iter()
        .map(|index| project.listed()[index].name().unwrap().to_owned())
        .collect()
}

fn condition(issues: &[Diagnostic]) -> Vec<&str> {
    issues.iter().map(|issue| issue.condition()).collect()
}

#[test]
fn listings_follow_section_run_order_and_names_are_their_domains() {
    let study = study();
    let project = read(&study);
    assert_eq!(names(&project, 0..3), ["DM", "ADAE", "ADSL"]);
    let listed = &project.listed()[1];
    assert_eq!(listed.listing(), "adam.specs[0]");
    assert_eq!(listed.section(), Submission::Adam);
    assert_eq!(listed.identity(), study.path("adam/adae.yaml"));
    assert_eq!(listed.path(), study.path("adam/adae.yaml"));
    assert_eq!(
        project.sections().collect::<Vec<_>>(),
        [Submission::Sdtm, Submission::Adam]
    );
    assert_eq!(project.define_sections().count(), 0);
    assert_eq!(project.find("ADSL").unwrap().name(), Some("ADSL"));
    let unknown = project.find("AE").unwrap_err();
    assert_eq!(condition(&unknown), ["unknown_specification_name"]);
    assert_eq!(
        unknown[0].context["available"],
        yamaa_adapters::json!(["ADAE", "ADSL", "DM"])
    );
    assert_eq!(
        unknown[0].context["source"],
        Value::String(study.path("project.yaml"))
    );
    assert!(project.duplicates().is_empty());

    // The same file listed twice shares its name with itself.
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nsdtm:\n  specs: [sdtm/dm.yaml]\nsend:\n  specs: [sdtm/../sdtm/dm.yaml]\n",
    );
    let duplicates = read(&study).duplicates();
    assert_eq!(condition(&duplicates), ["duplicate_specification_name"]);
    assert_eq!(duplicates[0].spec_paths, ["sdtm.specs[0]", "send.specs[0]"]);
    assert_eq!(
        duplicates[0].context["specifications"],
        yamaa_adapters::json!([study.path("sdtm/dm.yaml"), study.path("sdtm/dm.yaml")])
    );
}

#[test]
fn an_unreadable_project_fails_as_a_check_reports_it() {
    let study = Study::new();
    let project = study.path("project.yaml");
    let issues = Project::read(&project, Some(Language::Python), None)
        .err()
        .unwrap();
    // The caller's own path, named as given (FILE-7, #3321).
    assert_eq!(condition(&issues), ["resource_path_missing"]);
    assert!(issues[0].spec_paths.is_empty());
    assert_eq!(issues[0].context["path"], Value::String(project.clone()));
    let checked = yamaa_adapters::project_application::check(
        &study.path("spec.yaml"),
        &project,
        Language::Python,
    );
    assert_eq!(checked, issues);
}

#[test]
fn plans_order_their_scope_producer_first() {
    let study = study();
    let project = read(&study);
    let plan = project.plan(None);
    assert!(plan.refusal().is_empty());
    let order = plan.steps().iter().map(|step| step.listed());
    assert_eq!(names(&project, order), ["DM", "ADSL", "ADAE"]);
    assert!(plan.steps().iter().all(|step| step.refused().is_empty()));
    let adae = &plan.steps()[2];
    assert_eq!(names(&project, adae.producers()), ["ADSL"]);
    let adam = project.plan(Some(Submission::Adam));
    let order = adam.steps().iter().map(|step| step.listed());
    assert_eq!(names(&project, order), ["ADSL", "ADAE"]);
    // DM is outside the ADaM scope: ADSL has no run-scope producer.
    assert_eq!(adam.steps()[0].producers().count(), 0);
    assert!(project.plan(Some(Submission::Send)).steps().is_empty());
}

#[test]
fn blocked_consumers_name_each_failed_producer() {
    let study = study();
    let project = read(&study);
    let plan = project.plan(None);
    let adae = &plan.steps()[2];
    let dm = 0;
    assert!(adae.blocked(&project, |producer| producer == dm).is_empty());
    let blocked = adae.blocked(&project, |_| true);
    assert_eq!(condition(&blocked), ["producer_not_completed"]);
    assert_eq!(blocked[0].spec_paths, ["input.SRC.schema"]);
    assert_eq!(
        blocked[0].context["producer"],
        Value::String(study.path("adam/adsl.yaml"))
    );
    assert_eq!(
        blocked[0].context["entry"],
        Value::String(study.path("adam/adae.yaml"))
    );
}

#[test]
fn unlinked_inputs_cycles_and_collisions_are_project_findings() {
    let study = study();
    // ADSL reads DM's artifact without a link; ADAE and ADXX link each other.
    study.write(
        "adam/adsl.yaml",
        spec(
            "ADSL",
            "{path: ../out/dm.csv, types: {ID: int}}",
            "../out/adsl.csv",
        ),
    );
    study.write(
        "adam/adae.yaml",
        spec(
            "ADAE",
            "{path: ../out/adxx.csv, schema: adxx.yaml}",
            "../out/adae.csv",
        ),
    );
    study.write(
        "adam/adxx.yaml",
        spec(
            "ADXX",
            "{path: ../out/adae.csv, schema: adae.yaml}",
            "../out/adxx.csv",
        ),
    );
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nsdtm:\n  specs: [sdtm/dm.yaml]\nadam:\n  specs: [adam/adsl.yaml, adam/adae.yaml, adam/adxx.yaml]\n",
    );
    let project = read(&study);
    let plan = project.plan(None);
    assert!(plan.refusal().is_empty());
    let refused = |name: &str| {
        let step = plan
            .steps()
            .iter()
            .find(|step| project.listed()[step.listed()].name() == Some(name))
            .unwrap();
        step.refused().to_vec()
    };
    assert!(refused("DM").is_empty());
    let adsl = refused("ADSL");
    assert_eq!(condition(&adsl), ["unlinked_producer"]);
    assert_eq!(adsl[0].spec_paths, ["input.SRC.path"]);
    assert_eq!(
        adsl[0].context["path"],
        Value::String("../out/dm.csv".into())
    );
    for name in ["ADAE", "ADXX"] {
        let cycle = refused(name);
        assert_eq!(condition(&cycle), ["producer_workflow_cycle"], "{name}");
        assert_eq!(cycle[0].spec_paths, ["input.SRC.schema"]);
    }
    // In the ADaM scope alone, DM's file is an ordinary source.
    let adam = project.plan(Some(Submission::Adam));
    assert!(adam.steps().iter().any(|step| {
        project.listed()[step.listed()].name() == Some("ADSL") && step.refused().is_empty()
    }));
    // A project check reports the same unlinked input with every listing.
    let check = project.check_findings();
    assert_eq!(condition(&check), ["unlinked_producer"]);

    // Two run-scope files at one location refuse the whole run.
    study.write(
        "adam/adsl.yaml",
        spec("ADSL", RAW, "../out/dm.csv.report.csv"),
    );
    let project = read(&study);
    let refusal = project.plan(None).refusal().to_vec();
    assert_eq!(condition(&refusal), ["artifact_path_collision"]);
    assert_eq!(refusal[0].spec_paths, ["output.report", "output.path"]);
    assert_eq!(
        refusal[0].context["specifications"],
        yamaa_adapters::json!([study.path("sdtm/dm.yaml"), study.path("adam/adsl.yaml")])
    );
    // Outside the SDTM scope the collision is not part of the run.
    assert!(project.plan(Some(Submission::Sdtm)).refusal().is_empty());
    assert_eq!(
        condition(&project.check_findings()),
        ["artifact_path_collision"]
    );
}

/// Build ADSL alone, offering `held` artifacts for its DM link. A link
/// supplies only the contract; the input is read from its own path.
fn saved_build(
    study: &Study,
    held: Option<HeldArtifacts>,
) -> (bool, Option<SourceOrigin>, Vec<Diagnostic>) {
    let prepared = file_workflow::select_request(
        yamaa_core::build::Request {
            specification: &study.path("adam/adsl.yaml"),
            environment: None,
        },
        Some(Language::Python),
    )
    .unwrap_or_else(|error| {
        panic!(
            "{:?}",
            yamaa_adapters::project_application::selection_failure(&error)
        )
    });
    let held = held.map(Arc::new);
    let run = || {
        let mut run = prepared.run_scope(None);
        if let Some(held) = &held {
            run.hold(held.clone());
        }
        run
    };
    let origin = prepared
        .build(&mut run(), &mut NoActivation, scope())
        .dataset
        .sources
        .first()
        .and_then(|source| source.read.origin);
    let (_, result) = file_application::run(
        &prepared,
        &mut run(),
        &mut NoActivation,
        |_| file_application::Classification::Opaque,
        id(),
        scope(),
    );
    let issues = match result {
        Ok(domain) => domain.issues(),
        Err(issues) => issues,
    };
    (issues.is_empty(), origin, issues)
}

#[test]
fn saved_links_read_held_or_saved_artifacts_under_the_producer_contract() {
    let study = study();
    let dm = study.path("sdtm/dm.yaml");
    let artifact = study.path("out/dm.csv");
    // A held artifact is read without the file, as generated bytes.
    let mut held = HeldArtifacts::new();
    held.insert(dm.clone(), artifact.clone(), Arc::from(&b"ID\n1\n2\n"[..]));
    let (accepted, origin, issues) = saved_build(&study, Some(held));
    assert!(accepted, "{issues:?}");
    assert_eq!(origin, Some(SourceOrigin::Generated));
    // Without one, the missing saved file is a clear failure.
    let (accepted, _, issues) = saved_build(&study, None);
    assert!(!accepted);
    assert_eq!(condition(&issues), ["resource_path_missing"]);
    assert_eq!(issues[0].spec_paths, ["input.SRC.path"]);
    // A held artifact built for another location is not this link's.
    let mut elsewhere = HeldArtifacts::new();
    elsewhere.insert(dm, study.path("out/other.csv"), Arc::from(&b"ID\n1\n"[..]));
    let (accepted, _, _) = saved_build(&study, Some(elsewhere));
    assert!(!accepted);
    // The saved file is read as a resource and checked against the contract.
    study.write("out/dm.csv", "ID\n1\n2\n");
    let (accepted, origin, issues) = saved_build(&study, None);
    assert!(accepted, "{issues:?}");
    assert_eq!(origin, Some(SourceOrigin::Resource));
    study.write("out/dm.csv", "ID,EXTRA\n1,x\n");
    let (accepted, _, issues) = saved_build(&study, None);
    assert!(!accepted);
    assert_eq!(condition(&issues), ["producer_contract_mismatch"]);
}

fn domain(study: &Study, name: &str) -> Domain {
    yamaa_adapters::file_application::domain(
        Language::Python,
        yamaa_core::domain_entry::Request {
            specification: &study.path(name),
            environment: None,
        },
        id(),
    )
    .unwrap_or_else(|issues| panic!("{issues:?}"))
}

#[test]
fn report_only_saves_write_the_report_file_and_no_primary_artifact() {
    let study = study();
    let mut dm = domain(&study, "sdtm/dm.yaml");
    assert!(dm.artifact().is_some());
    assert!(dm.save_report().unwrap());
    assert!(study.0.join("out/dm.csv.report.csv").exists());
    assert!(!study.0.join("out/dm.csv").exists());
    assert!(dm.save().unwrap());
    assert_eq!(fs::read(study.0.join("out/dm.csv")).unwrap(), b"ID\n1\n2\n");

    let mut adsl = domain(&study, "adam/adsl.yaml");
    assert!(adsl.output().is_some(), "{:?}", adsl.issues());
    assert!(adsl.save_report().unwrap());
    assert!(study.0.join("out/adsl.csv.report.csv").exists());
    assert!(!study.0.join("out/adsl.csv").exists());
    assert!(adsl.save().unwrap());
    assert!(study.0.join("out/adsl.csv").exists());

    // A specification without a report file has nothing to save alone.
    study.write(
        "sdtm/ae.yaml",
        spec("AE", RAW, "../out/ae.csv").edit(", report: ../out/ae.csv.report.csv", ""),
    );
    assert!(domain(&study, "sdtm/ae.yaml").save_report().is_err());
}

#[test]
fn the_preflight_refuses_two_results_that_write_one_file() {
    // The files one save of `results` writes, each result's report alone
    // when it is marked so, in order.
    fn files<'a>(results: &[(&'a Domain, bool)]) -> Vec<publication::File<'a>> {
        results
            .iter()
            .flat_map(|(result, report_only)| result.plan(*report_only).unwrap().unwrap().0)
            .collect()
    }
    let study = study();
    study.write("sdtm/ae.yaml", spec("AE", RAW, "../out/dm.csv"));
    let dm = domain(&study, "sdtm/dm.yaml");
    let ae = domain(&study, "sdtm/ae.yaml");
    // AE's report is DM's report: the third file names the first's target.
    let refused = publication::stage(&files(&[(&dm, false), (&ae, false)]))
        .err()
        .unwrap();
    assert!(matches!(refused.cause, Cause::Collision { earlier: 0 }));
    assert_eq!((refused.file, refused.committed), (2, 0));
    // Report files alone do not collide.
    study.write("sdtm/ae.yaml", spec("AE", RAW, "../out/ae.csv"));
    let ae = domain(&study, "sdtm/ae.yaml");
    // ADSL fails without DM's saved artifact; only its report file is a
    // target, and it names its own location.
    let adsl = domain(&study, "adam/adsl.yaml");
    assert!(adsl.output().is_none());
    publication::stage(&files(&[(&dm, true), (&ae, true), (&adsl, false)]))
        .unwrap()
        .abort();
    // A result whose report file is one already admitted is refused.
    study.write(
        "sdtm/ae.yaml",
        spec("AE", RAW, "../out/ae.csv").edit(
            "report: ../out/ae.csv.report.csv",
            "report: ../out/adsl.csv.report.csv",
        ),
    );
    let ae = domain(&study, "sdtm/ae.yaml");
    let refused = publication::stage(&files(&[(&dm, true), (&adsl, false), (&ae, true)]))
        .err()
        .unwrap();
    assert!(matches!(refused.cause, Cause::Collision { earlier: 1 }));
    assert_eq!(fs::read_dir(study.0.join("out")).unwrap().count(), 0);
}
