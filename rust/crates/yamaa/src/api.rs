//! The host API both bindings expose: operations on a specification or a
//! project path. Hosts keep the paths in their own lazy objects; every call
//! here rereads the files, and every result is a fixed snapshot that writes
//! nothing until `save`.

use std::sync::Arc;

use yamaa_adapters::issue_rows::Issue;
use yamaa_adapters::project_scope::{identify, Project};
use yamaa_core::project_environment::Submission as Section;

use crate::cancel::CancellationToken;
use crate::diagnostic::{context_value_from_core, ContextValue, Report};
use crate::engine::{Build, DefineBuild, DefineRequest, Engine, Request, SaveOutcome};
use crate::error::Error;
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
        self.build(&request(specification, project), token)
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

    /// Publish the sections in the order `sdtm`, `adam`, `send`; the first
    /// section that fails stops the save.
    pub fn save(&mut self) -> Result<(), Failure> {
        if !self.refusal.is_empty() {
            return Err(Failure::from_issues(
                "the submission was refused; see issues",
                &self.refusal,
            ));
        }
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
        let read = Project::read(project, self.shared().language).map_err(|issues| {
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
        let read = match Project::read(project, self.shared().language) {
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
    /// failed specification stops only its consumers. Project problems come
    /// back as issues of a run that built nothing; only an unknown section
    /// (`Error::Rejected`) and cancellation fail the call.
    pub fn project_run(
        &self,
        project: &str,
        section: Option<&str>,
        token: &CancellationToken,
    ) -> Result<ProjectRun, Error> {
        if token.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let section = self::section(section)?;
        let read = match Project::read(project, self.shared().language) {
            Ok(read) => read,
            Err(issues) => return Ok(ProjectRun::refused(issues)),
        };
        let plan = read.plan(section);
        if !plan.refusal().is_empty() {
            return Ok(ProjectRun::refused(plan.refusal().to_vec()));
        }
        let mut held = yamaa_adapters::file_producer_build::HeldArtifacts::new();
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
                self.build_in_project(
                    &Request::new(listed.path()).with_environment(project),
                    Arc::new(held.clone()),
                    token,
                )?
            };
            let artifact = build.artifact();
            succeeded[step.listed()] = Some(artifact.is_some());
            if let (Some(bytes), Some(location)) = (artifact, listed.artifact_location()) {
                held.insert(listed.identity().to_owned(), location.to_owned(), bytes);
            }
            steps.push(ProjectStep {
                name: listed.name().map(str::to_owned),
                identity: listed.identity().to_owned(),
                run: Run { build },
            });
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
        let read = match Project::read(project, self.shared().language) {
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
