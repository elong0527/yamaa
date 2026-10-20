//! The host API both bindings expose: operations on a specification or a
//! project path. Hosts keep the paths in their own lazy objects; every call
//! here rereads the files, and every result is a fixed snapshot that writes
//! nothing until `save`. A lazy object also keeps a [`Plans`] holder, so a
//! later run reuses what an earlier run compiled once the run-start check
//! finds the files unchanged.
//!
//! Every operation produces one outcome: its members in run order and the
//! project-level issues that refused the whole operation. A specification run
//! is a producer graph of one member; a project run is a graph of every
//! specification in its scope, producer first. [`Check`], [`Run`],
//! [`ProjectRun`] and [`Submission`] are views of the outcome, and each
//! computes its [`Status`] from the issues and the build, never storing it.
//! Each prints its [`Summary`] (D37), and a refusal's message is D39's error
//! text, both rendered by [`crate::text`].

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use yamaa_adapters::project_scope::identify;
use yamaa_core::application_issue::{Fault, Stage};
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::project_environment::Submission as Section;

use crate::cancel::CancellationToken;
use crate::diagnostic::{context_value_from_core, ContextValue, Report};
use crate::engine::{Build, DefineRequest, Engine, Request, SaveOutcome};
use crate::error::Error;
use crate::outcome::{fails, Member, Outcome, Work};
use crate::plan::Plan;
use crate::table::Table;
use crate::text::{self, Action, Head, MemberLine, Summary};

/// A refused operation: D39's error text and the issues that explain it.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Failure {
    /// D39's error text: the call that refused, the first issue's line and
    /// the count of the other issues, such as `cannot save ADAE: column ASTDY
    /// reads TRTSDT, which is declared after it [forward_reference, COLUMN-5]
    /// (and 2 more issues)`. Only ASCII; see [`crate::text`].
    pub message: String,
    /// The issues table both hosts attach to their error (D28).
    pub issues: Table,
    /// The same issues as typed records, first issue first, for the command
    /// line's printed lines and JSON.
    pub diagnostics: Vec<crate::diagnostic::Diagnostic>,
}

impl Failure {
    /// The refusal of `action`, which `report`'s issues explain.
    pub fn new(action: Action<'_>, report: &Report) -> Self {
        Self {
            message: text::error_text(action, &report.diagnostics, None),
            issues: report.table(),
            diagnostics: report.diagnostics.clone(),
        }
    }

    /// The refusal of `action` for `error`: the issues of an engine refusal,
    /// or, for an error that carries none, the error's own text after the
    /// prefix.
    pub fn from_error(action: Action<'_>, error: &Error) -> Self {
        match error {
            Error::Rejected(report) => Self::new(action, report),
            other => {
                let report = Report::default();
                Self {
                    message: text::error_text(action, &[], Some(&other.to_string())),
                    issues: report.table(),
                    diagnostics: Vec::new(),
                }
            }
        }
    }

    fn from_issues(action: Action<'_>, issues: &[Diagnostic]) -> Self {
        Self::new(action, &Report::from_issues(issues))
    }

    /// A refusal that one registered finding explains.
    fn from_finding(action: Action<'_>, finding: Diagnostic) -> Self {
        Self::from_issues(action, std::slice::from_ref(&finding))
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Failure {}

/// The status of a result or of one member of it (P-STATUS-1). Rust computes
/// it from the issues and the build; it is never stored. A warning never
/// changes it, and `status` describes the operation, not a later save.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Everything the operation set out to build was built, with no
    /// error-severity issue.
    Succeeded,
    /// Anything else.
    Failed,
    /// A project-run member that did not run because a run-scope producer
    /// failed or was skipped (PROJECT-11).
    Skipped,
}

impl Status {
    /// The status as written: `succeeded`, `failed` or `skipped`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    /// `succeeded` exactly when every member succeeded and no issue fails
    /// the whole operation.
    pub(crate) fn of_all(issues: &[Diagnostic], members: impl IntoIterator<Item = Status>) -> Self {
        if !fails(issues) && members.into_iter().all(|status| status == Self::Succeeded) {
            Self::Succeeded
        } else {
            Self::Failed
        }
    }
}

/// The static check of a specification or a project: what is wrong, read
/// without study data or project code.
pub struct Check {
    outcome: Outcome,
    /// The path as given.
    subject: String,
}

impl Check {
    /// What is wrong, one row per finding; empty when nothing is. A project
    /// check lists the project findings first, then each specification's.
    pub fn issues(&self) -> Table {
        Report::from_issues(&self.outcome.issues()).table()
    }

    /// The issues as typed records, in the table's order.
    pub fn diagnostics(&self) -> Vec<crate::diagnostic::Diagnostic> {
        Report::from_issues(&self.outcome.issues()).diagnostics
    }

    /// `succeeded` when the check found no error-severity issue.
    pub fn status(&self) -> Status {
        self.outcome.status()
    }

    /// The printed text (D37, P-PRINT-1): `yamaa check of <path>: <status>,
    /// <n> issues`, naming the path as given, then the first issue.
    pub fn summary(&self) -> Summary {
        let head = Head {
            kind: "check",
            subject: &self.subject,
            status: self.status(),
            size: None,
        };
        text::summary(head, &[], &self.diagnostics())
    }
}

/// One specification run, or one member of a project run: its output
/// dataset, issues and run report.
pub struct Run {
    build: Build,
    /// What the run's summary names: a project run member's name, or the
    /// dataset name once read, or the path as given.
    subject: String,
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

    /// The issues as typed records, in the table's order.
    pub fn diagnostics(&self) -> Vec<crate::diagnostic::Diagnostic> {
        self.build.report().diagnostics.clone()
    }

    /// What the run read, counted and checked; missing when the run never
    /// began or, in a producer graph, when the entry never ran.
    pub fn report(&self) -> Option<&Table> {
        self.build.run_report()
    }

    /// `succeeded` when the run built its output with no error-severity
    /// issue; `skipped` for a project-run member that did not run because a
    /// run-scope producer failed or was skipped; `failed` otherwise.
    pub fn status(&self) -> Status {
        self.build.status()
    }

    /// The printed text (D37, P-PRINT-1): `yamaa run of <subject>: <status>,
    /// <size>, <n> issues`, then the first issue. The subject is the dataset
    /// name once read, a project run member's name, or else the path as
    /// given; the size is `<r> rows x <c> columns`, or `no output`.
    pub fn summary(&self) -> Summary {
        let head = Head {
            kind: "run",
            subject: &self.subject,
            status: self.status(),
            size: Some(text::output_size(self.output())),
        };
        text::summary(head, &[], &self.build.report().diagnostics)
    }

    /// Write the declared report file and the output dataset. A failed run
    /// writes only its report file and fails with the run's issues.
    pub fn save(&mut self) -> Result<(), Failure> {
        let action = Action::Save(&self.subject);
        match self.build.save() {
            Ok(SaveOutcome::Saved) => Ok(()),
            Ok(_) => Err(Failure::new(action, self.build.report())),
            Err(error) => Err(Failure::from_error(action, &error)),
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

/// One member of a producer graph: the specification to build, with its
/// project, and what decides whether it builds.
struct Node<'a> {
    name: Option<String>,
    identity: Option<String>,
    request: Request<'a>,
    /// What the request resolves to now, for a kept plan.
    plan: Option<PlanKey>,
    /// Findings that stop the member before it builds.
    refused: Vec<Diagnostic>,
    /// The producers it reads, by node, each with the finding the member
    /// reports when that producer did not complete.
    producers: Vec<(usize, Diagnostic)>,
    /// The producer identity and location under which consumers read the
    /// member's artifact.
    artifact: Option<(String, String)>,
}

impl Engine {
    /// Build every member of a producer graph once, in order. A producer
    /// hands its artifact to its consumers in memory; a member whose own
    /// findings refuse it, or whose producer did not complete, does not
    /// build, and a user interrupt in project code stops the graph after
    /// that member. Only cancellation fails the call.
    fn run_graph(
        &self,
        nodes: Vec<Node<'_>>,
        plans: &Plans,
        token: &CancellationToken,
    ) -> Result<Outcome, Error> {
        let mut held = yamaa_adapters::file_resources::HeldArtifacts::new();
        let mut succeeded = vec![false; nodes.len()];
        let mut members = Vec::with_capacity(nodes.len());
        for (position, node) in nodes.into_iter().enumerate() {
            let blocked = node
                .producers
                .into_iter()
                .filter(|(producer, _)| !succeeded[*producer])
                .map(|(_, finding)| finding)
                .collect::<Vec<_>>();
            let build = if !node.refused.is_empty() {
                self.refused_build(node.refused)
            } else if !blocked.is_empty() {
                self.refused_build(blocked)
            } else {
                self.build_reusing(
                    plans,
                    &node.request,
                    node.plan,
                    Some(Arc::new(held.clone())),
                    token,
                )?
            };
            let artifact = build.artifact();
            succeeded[position] = artifact.is_some();
            if let (Some(bytes), Some((producer, location))) = (artifact, node.artifact) {
                held.insert(producer, location, bytes);
            }
            // A user interrupt in project code stops the graph here, so no
            // later member builds or calls project code.
            let interrupted = build.interrupt().is_some();
            let subject = node
                .name
                .clone()
                .or_else(|| build.name().map(str::to_owned))
                .unwrap_or_else(|| node.request.specification.to_owned());
            members.push(Member {
                name: node.name,
                identity: node.identity,
                work: Work::Built(Box::new(Run { build, subject })),
            });
            if interrupted {
                break;
            }
        }
        Ok(Outcome {
            members,
            issues: Vec::new(),
        })
    }

    /// Check one specification without reading study data or running project
    /// code, with the project's codelists and functions when it has one. Only
    /// cancellation fails the call.
    pub fn spec_check(
        &self,
        specification: &str,
        project: Option<&str>,
        token: &CancellationToken,
    ) -> Result<Check, Error> {
        let issues = crate::internal::run_check(self.shared(), specification, project, token)?;
        Ok(Check {
            outcome: Outcome {
                members: vec![Member {
                    name: None,
                    identity: None,
                    work: Work::Checked(issues),
                }],
                issues: Vec::new(),
            },
            subject: specification.to_owned(),
        })
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
        let node = Node {
            name: None,
            identity: None,
            request,
            plan: identity,
            refused: Vec::new(),
            producers: Vec::new(),
            artifact: None,
        };
        let outcome = self.run_graph(vec![node], plans, token)?;
        let member = outcome
            .members
            .into_iter()
            .next()
            .expect("a graph of one member builds it");
        match member.work {
            Work::Built(run) => Ok(*run),
            Work::Checked(_) | Work::Composed(..) => unreachable!("a graph builds its members"),
        }
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
            .map_err(|error| Failure::from_error(Action::Resolve(specification), &error))?;
        resolved_value(specification, resolved.to_value())
    }
}

/// The resolved tree as the artifact's codec reads it. Facts the engine
/// cannot represent are an engine fault, refused with `engine_rejected`.
fn resolved_value(
    specification: &str,
    tree: Result<yamaa_core::json::Value, Error>,
) -> Result<ContextValue, Failure> {
    let tree = tree.map_err(|_| {
        Failure::from_issues(
            Action::Resolve(specification),
            &yamaa_adapters::file_application::rejected(
                Stage::Resolve,
                Fault::ArtifactRepresentation,
            ),
        )
    })?;
    Ok(context_value_from_core(&tree))
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
            let mut finding =
                yamaa_core::application_issue::rejected(Stage::Project, Fault::UnknownSection);
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
            Error::Rejected(Report::from_issues(&[finding]))
        })
}

/// What a project operation's summary names: the project path as given,
/// then `, section <s>` when a section was named.
fn project_subject(project: &str, section: Option<&str>) -> String {
    match section {
        Some(section) => format!("{project}, section {section}"),
        None => project.to_owned(),
    }
}

/// Every specification of one project run scope, built once in run order.
pub struct ProjectRun {
    outcome: Outcome,
    subject: String,
}

impl ProjectRun {
    /// The names of the run's specifications, in run order.
    pub fn names(&self) -> Vec<String> {
        self.outcome
            .members
            .iter()
            .filter_map(|member| member.name.clone())
            .collect()
    }

    /// The run of the specification named `name`.
    pub fn get(&self, name: &str) -> Option<&Run> {
        self.outcome
            .members
            .iter()
            .find(|member| member.name.as_deref() == Some(name))
            .and_then(Member::run)
    }

    /// The run of the specification named `name`, or the refusal every host
    /// reports for a name the run does not have: `unknown_specification_name`,
    /// naming every name the run has (PROJECT-6).
    pub fn member(&self, name: &str) -> Result<&Run, Failure> {
        self.get(name).ok_or_else(|| {
            let names = self.names();
            let available = names.iter().map(String::as_str).collect::<Vec<_>>();
            Failure::from_finding(
                Action::Select(name),
                yamaa_core::project_scope::unknown_name(name, &available),
            )
        })
    }

    /// Every run's issues in run order, after any project finding that
    /// refused the run. Each names its specification as its `entry`.
    pub fn issues(&self) -> Table {
        Report::from_issues(&self.outcome.issues()).table()
    }

    /// The issues as typed records, in the table's order.
    pub fn diagnostics(&self) -> Vec<crate::diagnostic::Diagnostic> {
        Report::from_issues(&self.outcome.issues()).diagnostics
    }

    /// The printed text (D37, P-PRINT-1): `yamaa project run of <path>[,
    /// section <s>]: <status>, <k> specifications, <n> issues`, then one line
    /// per specification in run order, then the first issue. A member line
    /// gives the name, the status, the size and the issue count, and a
    /// member's own summary is its run's, [`Run::summary`].
    pub fn summary(&self) -> Summary {
        let members = self
            .outcome
            .members
            .iter()
            .filter_map(|member| {
                let run = member.run()?;
                Some(MemberLine {
                    name: run.subject.clone(),
                    status: run.status(),
                    size: text::output_size(run.output()),
                    issues: member.issues().len(),
                })
            })
            .collect::<Vec<_>>();
        let head = Head {
            kind: "project run",
            subject: &self.subject,
            status: self.status(),
            size: Some(text::specifications(self.outcome.members.len())),
        };
        text::summary(head, &members, &self.diagnostics())
    }

    /// The runs' reports stacked in run order; each row names its
    /// specification's artifact. A specification that did not run adds none.
    pub fn report(&self) -> Table {
        let reports = self
            .outcome
            .members
            .iter()
            .filter_map(|member| member.run()?.report().map(Table::public))
            .collect::<Vec<_>>();
        Table::from_public(
            yamaa_adapters::project_scope::stack_reports(&reports)
                .expect("every run report has the run report columns"),
        )
    }

    /// `succeeded` when every specification succeeded and no project finding
    /// refused the run; a run whose scope is empty succeeds.
    pub fn status(&self) -> Status {
        self.outcome.status()
    }

    /// Validate every target, then publish in run order. When every
    /// specification succeeded, each publishes its declared report and then
    /// its primary artifact. When any failed, only the report files of the
    /// specifications that ran are written, and the save fails with every
    /// run's issues. The first file that cannot be written stops the save. A
    /// run that built no specification fails with `nothing_to_save`
    /// (PROJECT-12).
    pub fn save(&mut self) -> Result<(), Failure> {
        let action = Action::Save(&self.subject);
        let outcome = &mut self.outcome;
        if !outcome.issues.is_empty() {
            return Err(Failure::from_issues(action, &outcome.issues));
        }
        if outcome.members.is_empty() {
            return Err(Failure::from_finding(
                action,
                yamaa_core::project_scope::run_nothing_to_save(),
            ));
        }
        let failed = outcome
            .members
            .iter()
            .any(|member| member.run().is_none_or(|run| run.output().is_none()));
        let mut preflight = yamaa_adapters::project_scope::Preflight::new();
        for member in &outcome.members {
            let Some(run) = member.run() else { continue };
            if let Err(mut issues) = run.build.preflight(&mut preflight, failed) {
                if let Some(identity) = &member.identity {
                    identify(&mut issues, identity);
                }
                return Err(Failure::from_issues(action, &issues));
            }
        }
        drop(preflight);
        for member in &mut outcome.members {
            let Work::Built(run) = &mut member.work else {
                continue;
            };
            if failed {
                if run.build.save_report() == Ok(false) {
                    return Err(Failure::from_issues(action, &member.issues()));
                }
                continue;
            }
            match run.build.save() {
                Ok(SaveOutcome::Saved) => {}
                Ok(_) => return Err(Failure::from_issues(action, &member.issues())),
                Err(error) => return Err(Failure::from_error(action, &error)),
            }
        }
        if failed {
            return Err(Failure::from_issues(action, &outcome.issues()));
        }
        Ok(())
    }
}

/// One Define-XML composition per section, with its Dataset-JSON files.
pub struct Submission {
    outcome: Outcome,
    subject: String,
}

impl Submission {
    /// Every section's issues in section order, after any project finding.
    pub fn issues(&self) -> Table {
        Report::from_issues(&self.outcome.issues()).table()
    }

    /// The issues as typed records, in the table's order.
    pub fn diagnostics(&self) -> Vec<crate::diagnostic::Diagnostic> {
        Report::from_issues(&self.outcome.issues()).diagnostics
    }

    /// The printed text (D37, P-PRINT-1): `yamaa submission of <path>[,
    /// section <s>]: <status>, <d> define.xml and <t> transport files, <n>
    /// issues`, then the first issue. The counts are the documents of the
    /// sections that composed: each holds one define.xml and its transport
    /// files.
    pub fn summary(&self) -> Summary {
        let (defines, transports) = self
            .outcome
            .members
            .iter()
            .filter_map(|member| match &member.work {
                Work::Composed(define) if define.composed().is_empty() => Some(define.transports()),
                _ => None,
            })
            .fold((0, 0), |(defines, transports), files| {
                (defines + 1, transports + files)
            });
        let head = Head {
            kind: "submission",
            subject: &self.subject,
            status: self.status(),
            size: Some(text::submission_size(defines, transports)),
        };
        text::summary(head, &[], &self.diagnostics())
    }

    /// `succeeded` when every selected section composed with no issue and no
    /// project finding refused the submission.
    pub fn status(&self) -> Status {
        self.outcome.status()
    }

    /// Publish the sections in the order `sdtm`, `adam`, `send`, after
    /// checking that no two sections write one file; the first section that
    /// fails stops the save. A submission that composed no section fails with
    /// `nothing_to_save` (PROJECT-13).
    pub fn save(&mut self) -> Result<(), Failure> {
        let action = Action::Save(&self.subject);
        let outcome = &mut self.outcome;
        if !outcome.issues.is_empty() {
            return Err(Failure::from_issues(action, &outcome.issues));
        }
        if outcome.members.is_empty() {
            return Err(Failure::from_finding(
                action,
                yamaa_core::project_scope::submission_nothing_to_save(),
            ));
        }
        let mut preflight = yamaa_adapters::project_scope::Preflight::new();
        for member in &outcome.members {
            let Work::Composed(define) = &member.work else {
                continue;
            };
            if let Err(issues) = define.preflight(&mut preflight) {
                return Err(Failure::from_issues(action, &issues));
            }
        }
        drop(preflight);
        for member in &mut outcome.members {
            let Work::Composed(define) = &mut member.work else {
                continue;
            };
            match define.save() {
                Ok(SaveOutcome::Saved) => {}
                Ok(_) => return Err(Failure::new(action, define.report())),
                Err(error) => return Err(Failure::from_error(action, &error)),
            }
        }
        Ok(())
    }
}

impl Engine {
    /// Path of the listed specification whose `domain` is `name`.
    /// Every refusal names the call as `cannot select <name>` (D39): a
    /// project that cannot be read, one that lists a name twice, or a name it
    /// does not list, whose issue names the listed names.
    pub fn project_spec(&self, project: &str, name: &str) -> Result<String, Failure> {
        let action = Action::Select(name);
        let read = crate::internal::read_project(self.shared(), project)
            .map_err(|issues| Failure::from_issues(action, &issues))?;
        let duplicates = read.duplicates();
        if !duplicates.is_empty() {
            return Err(Failure::from_issues(action, &duplicates));
        }
        match read.find(name) {
            Ok(listed) => Ok(listed.path().to_owned()),
            Err(issues) => Err(Failure::from_issues(action, &issues)),
        }
    }

    /// Static check of every listed specification with the project's codelists
    /// and functions, plus the project findings: shared names, inputs that
    /// read a listed artifact without a `schema` link, and colliding files.
    /// Only cancellation fails the call, with no result: a check cancelled
    /// between specifications returns none of the findings it gathered.
    pub fn project_check(&self, project: &str, token: &CancellationToken) -> Result<Check, Error> {
        if token.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let read = match crate::internal::read_project(self.shared(), project) {
            Ok(read) => read,
            Err(issues) => {
                return Ok(Check {
                    outcome: Outcome::refused(issues),
                    subject: project.to_owned(),
                })
            }
        };
        let mut members = Vec::with_capacity(read.listed().len());
        for listed in read.listed() {
            if token.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let issues = if listed.failure().is_empty() {
                crate::internal::run_check(self.shared(), listed.path(), Some(project), token)?
            } else {
                listed.failure().to_vec()
            };
            members.push(Member {
                name: listed.name().map(str::to_owned),
                identity: Some(listed.identity().to_owned()),
                work: Work::Checked(issues),
            });
        }
        Ok(Check {
            outcome: Outcome {
                members,
                issues: read.check_findings(),
            },
            subject: project.to_owned(),
        })
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
        let subject = project_subject(project, section);
        let section = self::section(section)?;
        let read = match crate::internal::read_project(self.shared(), project) {
            Ok(read) => read,
            Err(issues) => {
                return Ok(ProjectRun {
                    outcome: Outcome::refused(issues),
                    subject,
                })
            }
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
            return Ok(ProjectRun {
                outcome: Outcome::refused(plan.refusal().to_vec()),
                subject,
            });
        }
        // The run scope as a producer graph, in run order.
        let position = |listing: usize| {
            plan.steps()
                .iter()
                .position(|step| step.listed() == listing)
                .expect("an in-scope producer is a step of the run")
        };
        let nodes = plan
            .steps()
            .iter()
            .map(|step| {
                let listed = &read.listed()[step.listed()];
                Node {
                    name: listed.name().map(str::to_owned),
                    identity: Some(listed.identity().to_owned()),
                    request: Request::new(listed.path()).with_environment(project),
                    plan: identities[step.listed()].clone(),
                    refused: step.refused().to_vec(),
                    producers: step
                        .links(&read)
                        .into_iter()
                        .map(|(producer, finding)| (position(producer), finding))
                        .collect(),
                    artifact: listed
                        .artifact_location()
                        .map(|location| (listed.identity().to_owned(), location.to_owned())),
                }
            })
            .collect();
        Ok(ProjectRun {
            outcome: self.run_graph(nodes, plans, token)?,
            subject,
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
        let subject = project_subject(project, section);
        let selected = self::section(section)?;
        let read = match crate::internal::read_project(self.shared(), project) {
            Ok(read) => read,
            Err(issues) => {
                return Ok(Submission {
                    outcome: Outcome::refused(issues),
                    subject,
                })
            }
        };
        let duplicates = read.duplicates();
        if !duplicates.is_empty() {
            return Ok(Submission {
                outcome: Outcome::refused(duplicates),
                subject,
            });
        }
        let sections = match selected {
            Some(section) => vec![section],
            None => read.define_sections().collect(),
        };
        let mut members = Vec::with_capacity(sections.len());
        for section in sections {
            let mut request =
                DefineRequest::new(project, yamaa_core::project_scope::key(section), installed);
            if let Some(created) = created {
                request = request.with_created(created);
            }
            members.push(Member {
                name: Some(yamaa_core::project_scope::key(section).to_owned()),
                identity: None,
                work: Work::Composed(Box::new(self.define(&request, token)?)),
            });
        }
        Ok(Submission {
            outcome: Outcome {
                members,
                issues: Vec::new(),
            },
            subject,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::{Array, StringArray};

    /// The text cells of one column of an issues table, in row order.
    fn cells(table: &Table, column: &str) -> Vec<Option<String>> {
        table
            .batches()
            .iter()
            .flat_map(|batch| {
                let column = batch
                    .column_by_name(column)
                    .unwrap()
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap();
                (0..column.len())
                    .map(|row| (!column.is_null(row)).then(|| column.value(row).to_owned()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Resolved facts the engine cannot represent are refused as the
    /// registered engine fault, whose message names the stage and code.
    #[test]
    fn unrepresentable_resolved_facts_are_an_engine_fault() {
        let fault = yamaa_core::diagnostic::ConditionCode::ApplicationRejected.definition();
        let unwritten = Error::Internal {
            code: "resolved_serialization",
        };
        let failure = resolved_value("adam/adsl.yaml", Err(unwritten)).unwrap_err();
        assert_eq!(
            failure.message,
            format!(
                "cannot resolve adam/adsl.yaml: the engine refused the operation in its \
                 resolve stage: artifact_representation [{}]",
                fault.condition
            )
        );
        assert_eq!(
            cells(&failure.issues, "phase"),
            [Some(fault.phase.to_owned())]
        );
        assert_eq!(
            cells(&failure.issues, "condition"),
            [Some(fault.condition.to_owned())]
        );
        assert_eq!(cells(&failure.issues, "rule"), [None]);
        assert_eq!(
            cells(&failure.issues, "context"),
            [Some(
                r#"{"code":"artifact_representation","stage":"resolve"}"#.to_owned()
            )]
        );
        let tree = yamaa_core::json::Value::Object(
            [(
                "entry".to_owned(),
                yamaa_core::json::Value::String("spec.yaml".to_owned()),
            )]
            .into(),
        );
        assert_eq!(
            resolved_value("adam/adsl.yaml", Ok(tree)).unwrap(),
            ContextValue::Map(
                [(
                    "entry".to_owned(),
                    ContextValue::Text("spec.yaml".to_owned())
                )]
                .into()
            )
        );
    }
}
