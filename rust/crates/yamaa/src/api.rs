//! The host API both bindings expose: operations on a specification or a
//! project path. Hosts keep the paths in their own lazy objects; every call
//! here rereads the files, and every result is a fixed snapshot that writes
//! nothing until `save`. A lazy object also keeps a [`Plans`] holder, so a
//! later run reuses what an earlier run compiled once the run-start check
//! finds the files unchanged.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use yamaa_adapters::issue_rows::Issue;
use yamaa_adapters::project_scope::identify;
use yamaa_core::project_environment::Submission as Section;

use crate::cancel::CancellationToken;
use crate::diagnostic::{context_value_from_core, ContextValue, Report};
use crate::engine::{Build, DefineBuild, DefineRequest, Engine, Request, SaveOutcome};
use crate::error::Error;
use crate::plan::Plan;
use crate::table::Table;

/// A refused operation: a message for people and the issues that explain it.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Failure {
    pub message: String,
    pub issues: Table,
}

impl Failure {
    pub fn new(message: impl Into<String>, report: &Report) -> Self {
        Self {
            message: message.into(),
            issues: report.table(),
        }
    }

    pub fn from_error(context: &str, error: &Error) -> Self {
        match error {
            Error::Rejected(report) => Self::new(format!("{context}; see issues"), report),
            other => Self::new(format!("{context}: {other}"), &Report::default()),
        }
    }

    fn from_issues(message: impl Into<String>, issues: &[Issue]) -> Self {
        Self::new(message, &Report::from_issues(issues))
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Failure {}

/// One specification run: its output dataset, issues and run report.
pub struct Run {
    build: Build,
}

impl Run {
    /// The output dataset, missing when the run failed.
    pub fn output(&self) -> Option<&Table> {
        self.build.output()
    }

    /// What is wrong, one row per finding; empty when nothing is.
    pub fn issues(&self) -> Table {
        self.build.report().table()
    }

    /// What the run read, counted and checked; missing when the run never
    /// began or, in a producer graph, when the entry never ran.
    pub fn report(&self) -> Option<&Table> {
        self.build.run_report()
    }

    /// Write the declared report file and the output dataset. A failed run
    /// writes only its report file and fails with the run's issues.
    pub fn save(&mut self) -> Result<(), Failure> {
        match self.build.save() {
            Ok(SaveOutcome::Saved) => Ok(()),
            Ok(_) => Err(Failure::new(
                "nothing was saved; see issues",
                self.build.report(),
            )),
            Err(error) => Err(Failure::from_error("cannot save a failed run", &error)),
        }
    }

    /// Borrow the underlying build for qualification evidence.
    #[doc(hidden)]
    pub fn build(&self) -> &Build {
        &self.build
    }
}

fn request<'a>(specification: &'a str, project: Option<&'a str>) -> Request<'a> {
    match project {
        Some(project) => Request::new(specification).with_environment(project),
        None => Request::new(specification),
    }
}

/// The identities of a specification and of its project file, resolved as
/// compilation resolves them: what a kept plan was compiled from.
pub(crate) type PlanKey = (String, Option<String>);

/// The compiled plans a host's lazy object keeps between runs (#2821), one
/// for each specification and project identity its runs compiled.
///
/// [`Engine::spec_run_reusing`] and [`Engine::project_run_reusing`] first
/// resolve the request's paths exactly as compilation does: absolute against
/// the current working directory, with each parent directory canonical. A
/// kept plan is reused only when those identities equal the ones it was
/// compiled from, so a relative path after a working-directory change, or a
/// path through a retargeted link, compiles anew (#3069). Its run-start check
/// must then find every file it was compiled from unchanged: the
/// specification, its layers, dictionaries and linked producing
/// specifications, the project file, the environment file, and the lock,
/// function definitions and codelists they declare. Otherwise the run
/// compiles again, exactly as a one-shot run does, and keeps the new plan; a
/// refused compilation keeps nothing. Every run reads its inputs and activates
/// the project functions afresh, so a reused run differs from a one-shot run
/// only in time.
///
/// A kept plan holds no project functions or observer: it runs through the
/// calling engine's. A holder is `Send` and `Sync`. Its lock is held only to
/// read or replace a kept plan, never across a compilation, a run or a
/// callback, and a replaced plan is dropped after the lock is released.
#[derive(Default)]
pub struct Plans {
    kept: Mutex<BTreeMap<PlanKey, Plan>>,
}

impl Plans {
    /// An empty holder: the first run of each specification compiles it.
    pub fn new() -> Self {
        Self::default()
    }

    fn kept(&self) -> MutexGuard<'_, BTreeMap<PlanKey, Plan>> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The plan kept for `identity`, if any.
    pub(crate) fn get(&self, identity: &PlanKey) -> Option<Plan> {
        self.kept().get(identity).cloned()
    }

    /// Keep `plan` for `identity`, or forget what `identity` kept when there
    /// is no plan to keep or its compilation was refused.
    pub(crate) fn replace(&self, identity: PlanKey, plan: Option<&Plan>) {
        let replaced = {
            let mut kept = self.kept();
            match plan.filter(|plan| plan.report().is_empty()) {
                Some(plan) => kept.insert(identity, plan.detached()),
                None => kept.remove(&identity),
            }
        };
        drop(replaced);
    }

    /// Forget every kept plan whose identity `keep` rejects.
    pub(crate) fn retain(&self, keep: impl Fn(&PlanKey) -> bool) {
        let dropped = {
            let mut kept = self.kept();
            let rejected = kept
                .keys()
                .filter(|identity| !keep(identity))
                .cloned()
                .collect::<Vec<_>>();
            rejected
                .into_iter()
                .filter_map(|identity| kept.remove(&identity))
                .collect::<Vec<_>>()
        };
        drop(dropped);
    }

    #[cfg(test)]
    pub(crate) fn identities(&self) -> Vec<PlanKey> {
        self.kept().keys().cloned().collect()
    }
}

impl Engine {
    /// Check one specification without reading study data or running project
    /// code, with the project's codelists and functions when it has one.
    pub fn spec_check(
        &self,
        specification: &str,
        project: Option<&str>,
        token: &CancellationToken,
    ) -> Table {
        self.check(&request(specification, project), token).table()
    }

    /// Build one specification in memory; nothing is written until `save`.
    pub fn spec_run(
        &self,
        specification: &str,
        project: Option<&str>,
        token: &CancellationToken,
    ) -> Result<Run, Error> {
        self.spec_run_reusing(specification, project, &Plans::new(), token)
    }

    /// [`Engine::spec_run`] through `plans`, as a host's specification object
    /// runs. The first run compiles the specification and keeps its plan; a
    /// later run reuses that plan when its paths still resolve to the files
    /// it was compiled from and the run-start check finds every one of them
    /// unchanged, and compiles again otherwise, so no run uses stale or moved
    /// files. See [`Plans`].
    pub fn spec_run_reusing(
        &self,
        specification: &str,
        project: Option<&str>,
        plans: &Plans,
        token: &CancellationToken,
    ) -> Result<Run, Error> {
        let request = request(specification, project);
        let identity = crate::internal::request_identity(self.shared(), &request);
        // A spec object names one specification: a plan kept for another
        // identity, such as the one its relative path named before the
        // working directory changed, is dropped.
        plans.retain(|kept| identity.as_ref() == Some(kept));
        self.build_reusing(plans, &request, identity, None, token)
            .map(|build| Run { build })
    }

    /// The specification after its layers are composed, as one structured
    /// tree: the normalized model, each field's origin and each resource.
    pub fn spec_resolve(
        &self,
        specification: &str,
        project: Option<&str>,
    ) -> Result<ContextValue, Failure> {
        let resolved = self
            .resolve_specification(&request(specification, project))
            .map_err(|error| Failure::from_error("cannot resolve the specification", &error))?;
        let text = resolved
            .to_json()
            .map_err(|error| Failure::from_error("cannot resolve the specification", &error))?;
        let value = yamaa_adapters::json_wire::decode(&text).map_err(|_| {
            Failure::new(
                "cannot resolve the specification: invalid resolved facts",
                &Report::default(),
            )
        })?;
        Ok(context_value_from_core(&value))
    }
}

/// The section a caller names: `sdtm`, `adam` or `send`; any other text is a
/// refused argument.
fn section(section: Option<&str>) -> Result<Option<Section>, Error> {
    let Some(key) = section else {
        return Ok(None);
    };
    yamaa_core::project_scope::section(key)
        .map(Some)
        .ok_or_else(|| {
            let mut finding = yamaa_core::application_issue::rejected("project", "unknown_section");
            finding.context.insert(
                "section".into(),
                yamaa_core::json::Value::String(key.into()),
            );
            finding.context.insert(
                "sections".into(),
                yamaa_core::json::Value::Array(
                    yamaa_core::project_scope::SECTIONS
                        .into_iter()
                        .map(|section| {
                            yamaa_core::json::Value::String(
                                yamaa_core::project_scope::key(section).into(),
                            )
                        })
                        .collect(),
                ),
            );
            let issue = Issue::from_core(finding).expect("an application refusal is portable");
            Error::Rejected(Report::from_issues(&[issue]))
        })
}

/// One specification of a project run: its name, when it has one, its
/// identity and its run.
struct ProjectStep {
    name: Option<String>,
    identity: String,
    run: Run,
}

/// The name a message uses for a step: its name, else its identity.
fn label(step: &ProjectStep) -> &str {
    step.name.as_deref().unwrap_or(&step.identity)
}

/// A step's issues, each naming its specification as its entry.
fn step_issues(step: &ProjectStep) -> Vec<Issue> {
    let mut issues = step.run.build.issue_rows();
    identify(&mut issues, &step.identity);
    issues
}

/// Every specification of one project run scope, built once in run order.
pub struct ProjectRun {
    steps: Vec<ProjectStep>,
    /// Project findings that refused the run before any specification built.
    refusal: Vec<Issue>,
}

impl ProjectRun {
    fn refused(refusal: Vec<Issue>) -> Self {
        Self {
            steps: Vec::new(),
            refusal,
        }
    }

    /// The names of the run's specifications, in run order.
    pub fn names(&self) -> Vec<String> {
        self.steps
            .iter()
            .filter_map(|step| step.name.clone())
            .collect()
    }

    /// The run of the specification named `name`.
    pub fn get(&self, name: &str) -> Option<&Run> {
        self.steps
            .iter()
            .find(|step| step.name.as_deref() == Some(name))
            .map(|step| &step.run)
    }

    /// The run of the specification named `name`, or the refusal every host
    /// reports for a name the run does not have.
    pub fn member(&self, name: &str) -> Result<&Run, Failure> {
        self.get(name).ok_or_else(|| {
            let names = self.names();
            Failure::new(
                if names.is_empty() {
                    format!("the project run has no specification named {name:?}; it built none")
                } else {
                    format!(
                        "the project run has no specification named {name:?}; it has {}",
                        names.join(", ")
                    )
                },
                &Report::default(),
            )
        })
    }

    /// Every run's issues in run order, after any project finding that
    /// refused the run. Each names its specification as its `entry`.
    pub fn issues(&self) -> Table {
        let mut issues = self.refusal.clone();
        for step in &self.steps {
            issues.extend(step_issues(step));
        }
        Report::from_issues(&issues).table()
    }

    /// The runs' reports stacked in run order; each row names its
    /// specification's artifact. A specification that did not run adds none.
    pub fn report(&self) -> Table {
        let reports = self
            .steps
            .iter()
            .filter_map(|step| step.run.report().map(Table::public))
            .collect::<Vec<_>>();
        Table::from_public(
            yamaa_adapters::project_scope::stack_reports(&reports)
                .expect("every run report has the run report columns"),
        )
    }

    /// Validate every target, then publish in run order. When every
    /// specification succeeded, each publishes its declared report and then
    /// its primary artifact. When any failed, only the report files of the
    /// specifications that ran are written, and the save fails with every
    /// run's issues. The first file that cannot be written stops the save.
    pub fn save(&mut self) -> Result<(), Failure> {
        if !self.refusal.is_empty() {
            return Err(Failure::from_issues(
                "the project run was refused; see issues",
                &self.refusal,
            ));
        }
        if self.steps.is_empty() {
            return Err(Failure::new(
                "the project run built no specification, so nothing was saved",
                &Report::default(),
            ));
        }
        let failed = self.steps.iter().any(|step| step.run.output().is_none());
        let mut preflight = yamaa_adapters::project_scope::Preflight::new();
        for step in &self.steps {
            if let Err(mut issues) = step.run.build.preflight(&mut preflight, failed) {
                identify(&mut issues, &step.identity);
                return Err(Failure::from_issues(
                    format!("cannot save {}; see issues", label(step)),
                    &issues,
                ));
            }
        }
        drop(preflight);
        for step in &mut self.steps {
            if failed {
                if step.run.build.save_report() == Ok(false) {
                    return Err(Failure::from_issues(
                        format!("cannot save the report of {}; see issues", label(step)),
                        &step_issues(step),
                    ));
                }
                continue;
            }
            let label = label(step).to_owned();
            match step.run.build.save() {
                Ok(SaveOutcome::Saved) => {}
                Ok(_) => {
                    return Err(Failure::from_issues(
                        format!("cannot save {label}; see issues"),
                        &step_issues(step),
                    ))
                }
                Err(error) => {
                    return Err(Failure::from_error(&format!("cannot save {label}"), &error))
                }
            }
        }
        if failed {
            let issues = self.steps.iter().flat_map(step_issues).collect::<Vec<_>>();
            return Err(Failure::from_issues(
                "a specification failed, so no primary artifact was saved; see issues",
                &issues,
            ));
        }
        Ok(())
    }
}

/// One Define-XML composition per section, with its Dataset-JSON files.
pub struct Submission {
    sections: Vec<(Section, DefineBuild)>,
    /// Project findings that refused the submission before any section.
    refusal: Vec<Issue>,
}

impl Submission {
    /// Every section's issues in section order, after any project finding.
    pub fn issues(&self) -> Table {
        let mut report = Report::from_issues(&self.refusal);
        for (_, define) in &self.sections {
            report
                .diagnostics
                .extend(define.report().diagnostics.iter().cloned());
        }
        report.table()
    }

    /// Publish the sections in the order `sdtm`, `adam`, `send`, after
    /// checking that no two sections write one file; the first section that
    /// fails stops the save.
    pub fn save(&mut self) -> Result<(), Failure> {
        if !self.refusal.is_empty() {
            return Err(Failure::from_issues(
                "the submission was refused; see issues",
                &self.refusal,
            ));
        }
        if self.sections.is_empty() {
            return Err(Failure::new(
                "the submission composed no section, so nothing was saved",
                &Report::default(),
            ));
        }
        let mut preflight = yamaa_adapters::project_scope::Preflight::new();
        for (section, define) in &self.sections {
            if let Err(issues) = define.preflight(&mut preflight) {
                return Err(Failure::from_issues(
                    format!(
                        "cannot save the {} submission; see issues",
                        yamaa_core::project_scope::key(*section)
                    ),
                    &issues,
                ));
            }
        }
        drop(preflight);
        for (section, define) in &mut self.sections {
            let key = yamaa_core::project_scope::key(*section);
            match define.save() {
                Ok(SaveOutcome::Saved) => {}
                Ok(_) => {
                    return Err(Failure::new(
                        format!("nothing was saved for the {key} submission; see issues"),
                        define.report(),
                    ))
                }
                Err(error) => {
                    return Err(Failure::from_error(
                        &format!("cannot save the {key} submission"),
                        &error,
                    ))
                }
            }
        }
        Ok(())
    }
}

impl Engine {
    /// Path of the listed specification whose `domain` is `name`.
    pub fn project_spec(&self, project: &str, name: &str) -> Result<String, Failure> {
        let read = crate::internal::read_project(self.shared(), project).map_err(|issues| {
            Failure::from_issues("cannot read the project; see issues", &issues)
        })?;
        let duplicates = read.duplicates();
        if !duplicates.is_empty() {
            return Err(Failure::from_issues(
                "the project lists one specification name more than once; see issues",
                &duplicates,
            ));
        }
        match read.find(name) {
            Ok(listed) => Ok(listed.path().to_owned()),
            Err(issues) => {
                let mut available = read
                    .listed()
                    .iter()
                    .filter_map(|listed| listed.name())
                    .collect::<Vec<_>>();
                available.sort_unstable();
                Err(Failure::from_issues(
                    format!(
                        "no specification named {name}; available: {}",
                        available.join(", ")
                    ),
                    &issues,
                ))
            }
        }
    }

    /// Static check of every listed specification with the project's codelists
    /// and functions, plus the project findings: shared names, inputs that
    /// read a listed artifact without a `schema` link, and colliding files.
    pub fn project_check(&self, project: &str, token: &CancellationToken) -> Table {
        if token.is_cancelled() {
            return Report::from_issues(&[crate::internal::cancelled_issue()]).table();
        }
        let read = match crate::internal::read_project(self.shared(), project) {
            Ok(read) => read,
            Err(issues) => return Report::from_issues(&issues).table(),
        };
        let mut issues = read.check_findings();
        for listed in read.listed() {
            if token.is_cancelled() {
                issues.push(crate::internal::cancelled_issue());
                break;
            }
            let mut rows = if listed.failure().is_empty() {
                crate::internal::run_check(self.shared(), listed.path(), Some(project), token)
            } else {
                listed.failure().to_vec()
            };
            identify(&mut rows, listed.identity());
            issues.extend(rows);
        }
        Report::from_issues(&issues).table()
    }

    /// Build every listed specification of `section` ("sdtm", "adam", "send"),
    /// or of all sections in that order. Each builds once, producer first; a
    /// producer in the run scope hands its artifact to its consumers in
    /// memory, and one outside it is read from its saved artifact file. A
    /// failed specification stops only its consumers, and a user interrupt in
    /// project code stops the run after that specification. Project problems
    /// come back as issues of a run that built nothing; only an unknown
    /// section (`Error::Rejected`) and cancellation fail the call.
    pub fn project_run(
        &self,
        project: &str,
        section: Option<&str>,
        token: &CancellationToken,
    ) -> Result<ProjectRun, Error> {
        self.project_run_reusing(project, section, &Plans::new(), token)
    }

    /// [`Engine::project_run`] through `plans`, as a host's project object
    /// runs. The project file is read afresh every time, so the run scope and
    /// order follow the current file, and each listed specification is
    /// identified from that read: the plan an earlier run kept for the same
    /// specification and project identities is reused under the rule of
    /// [`Engine::spec_run_reusing`]. Plans kept for anything the file no
    /// longer lists, or for another project file, are dropped.
    pub fn project_run_reusing(
        &self,
        project: &str,
        section: Option<&str>,
        plans: &Plans,
        token: &CancellationToken,
    ) -> Result<ProjectRun, Error> {
        if token.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let section = self::section(section)?;
        let read = match crate::internal::read_project(self.shared(), project) {
            Ok(read) => read,
            Err(issues) => return Ok(ProjectRun::refused(issues)),
        };
        // Identify every listed specification from this read, with the
        // project path resolved now rather than its raw spelling.
        let identities = read
            .listed()
            .iter()
            .map(|listed| {
                (listed.failure().is_empty())
                    .then(|| {
                        let request = Request::new(listed.path()).with_environment(project);
                        crate::internal::request_identity(self.shared(), &request)
                    })
                    .flatten()
            })
            .collect::<Vec<_>>();
        plans.retain(|kept| identities.iter().flatten().any(|identity| identity == kept));
        let plan = read.plan(section);
        if !plan.refusal().is_empty() {
            return Ok(ProjectRun::refused(plan.refusal().to_vec()));
        }
        let mut held = yamaa_adapters::file_resources::HeldArtifacts::new();
        // Per listing: whether its run succeeded, once it ran.
        let mut succeeded = vec![None; read.listed().len()];
        let mut steps = Vec::with_capacity(plan.steps().len());
        for step in plan.steps() {
            let listed = &read.listed()[step.listed()];
            let blocked = step.blocked(&read, |producer| succeeded[producer] != Some(true));
            let build = if !step.refused().is_empty() {
                self.refused_build(step.refused().to_vec())
            } else if !blocked.is_empty() {
                self.refused_build(blocked)
            } else {
                self.build_reusing(
                    plans,
                    &Request::new(listed.path()).with_environment(project),
                    identities[step.listed()].clone(),
                    Some(Arc::new(held.clone())),
                    token,
                )?
            };
            let artifact = build.artifact();
            succeeded[step.listed()] = Some(artifact.is_some());
            if let (Some(bytes), Some(location)) = (artifact, listed.artifact_location()) {
                held.insert(listed.identity().to_owned(), location.to_owned(), bytes);
            }
            // A user interrupt in project code stops the run here, so no
            // later specification builds or calls project code.
            let interrupted = build.interrupt().is_some();
            steps.push(ProjectStep {
                name: listed.name().map(str::to_owned),
                identity: listed.identity().to_owned(),
                run: Run { build },
            });
            if interrupted {
                break;
            }
        }
        Ok(ProjectRun {
            steps,
            refusal: Vec::new(),
        })
    }

    /// One define.xml per section, selected or all declared, plus
    /// Dataset-JSON where the section sets `datasets: json`. Without a
    /// section, every declared section that names `define` is composed.
    /// Project problems come back as issues; only an unknown section
    /// (`Error::Rejected`) and cancellation fail the call.
    pub fn project_submission(
        &self,
        project: &str,
        section: Option<&str>,
        created: Option<&str>,
        installed: &str,
        token: &CancellationToken,
    ) -> Result<Submission, Error> {
        if token.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let selected = self::section(section)?;
        let read = match crate::internal::read_project(self.shared(), project) {
            Ok(read) => read,
            Err(issues) => {
                return Ok(Submission {
                    sections: Vec::new(),
                    refusal: issues,
                })
            }
        };
        let duplicates = read.duplicates();
        if !duplicates.is_empty() {
            return Ok(Submission {
                sections: Vec::new(),
                refusal: duplicates,
            });
        }
        let sections = match selected {
            Some(section) => vec![section],
            None => read.define_sections().collect(),
        };
        let mut defines = Vec::with_capacity(sections.len());
        for section in sections {
            let mut request =
                DefineRequest::new(project, yamaa_core::project_scope::key(section), installed);
            if let Some(created) = created {
                request = request.with_created(created);
            }
            defines.push((section, self.define(&request, token)?));
        }
        Ok(Submission {
            sections: defines,
            refusal: Vec::new(),
        })
    }
}

#[cfg(all(test, feature = "yaml"))]
mod plan_tests {
    use super::*;
    use crate::{
        Argument, CallError, EngineOptions, FunctionError, FunctionHandle, FunctionIdentity,
        HostLanguage, LockContext, ProjectFunctions, Scalar, Signature,
    };

    struct NoFunctions;
    impl ProjectFunctions for NoFunctions {
        fn verify_lock(
            &self,
            _: &LockContext<'_>,
            _: &[FunctionIdentity],
        ) -> Result<(), FunctionError> {
            Ok(())
        }
        fn bind(
            &self,
            _: &FunctionIdentity,
            _: &Signature,
        ) -> Result<FunctionHandle, FunctionError> {
            unreachable!("these specifications call no project function")
        }
        fn invoke(&self, _: &FunctionHandle, _: &[Argument<'_>]) -> Result<Scalar, CallError> {
            unreachable!("these specifications call no project function")
        }
    }

    #[test]
    fn a_project_run_drops_the_plans_of_specifications_the_project_no_longer_lists() {
        const SPEC: &str = "schema_version: '1.0'\ndomain: DM\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int}}}\noutput: {path: dm.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: SRC.ID}\n";
        let root = std::env::temp_dir().join(format!("yamaa-plan-drop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let root = yamaa_adapters::canonical_path::canonicalize(root).unwrap();
        std::fs::write(root.join("dm.yaml"), SPEC).unwrap();
        std::fs::write(
            root.join("ae.yaml"),
            SPEC.replace("domain: DM", "domain: AE")
                .replace("dm.csv", "ae.csv"),
        )
        .unwrap();
        std::fs::write(root.join("input.csv"), "ID\n1\n").unwrap();
        let listing = |specs: &str| {
            std::fs::write(
                root.join("project.yaml"),
                format!("schema_version: '1.0'\nsdtm:\n  specs: [{specs}]\n"),
            )
            .unwrap();
        };
        let project = root.join("project.yaml").to_str().unwrap().to_owned();
        let engine = Engine::new(
            EngineOptions::new(HostLanguage::Python, "test".into(), "0".into())
                .with_functions(Arc::new(NoFunctions)),
        );
        let plans = Plans::new();
        let run = || {
            let runs = engine
                .project_run_reusing(&project, None, &plans, &CancellationToken::new())
                .unwrap();
            assert_eq!(runs.issues().rows(), 0);
        };
        listing("dm.yaml, ae.yaml");
        run();
        let names = |plans: &Plans| {
            plans
                .identities()
                .into_iter()
                .map(|(specification, project)| {
                    let name = |path: &str| path.rsplit('/').next().unwrap().to_owned();
                    (name(&specification), project.as_deref().map(name))
                })
                .collect::<Vec<_>>()
        };
        let kept = |name: &str| (name.to_owned(), Some("project.yaml".to_owned()));
        assert_eq!(names(&plans), [kept("ae.yaml"), kept("dm.yaml")]);
        // AE leaves the project: its plan goes, and DM's plan is compiled
        // again because the project file changed.
        listing("dm.yaml");
        run();
        assert_eq!(names(&plans), [kept("dm.yaml")]);
        std::fs::remove_dir_all(root).unwrap();
    }
}
