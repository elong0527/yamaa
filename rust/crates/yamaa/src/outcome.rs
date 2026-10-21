//! One operation's outcome: its members, in run order, and the project-level
//! issues that refused the whole operation. A specification run is a graph of
//! one member; a project run, check or submission has one member per
//! specification or section. The public views in [`crate::api`] read it, and
//! each computes its status from the issues and the work; nothing here stores
//! a status.
//!
//! [`save`] is the one save every result shares (D38, D74, P-API-1): it plans
//! every member's files, opens and stages all of them, and only then commits
//! them in publication order. Its refusals name the result they save (D39).

use std::ops::Range;

use yamaa_adapters::file_application::Admitted;
use yamaa_adapters::project_scope::identify;
use yamaa_adapters::publication::{self, Cause};
use yamaa_core::diagnostic::Diagnostic;

use crate::api::{Failure, Run, Status};
use crate::cancel::CancellationToken;
use crate::diagnostic::Severity;
use crate::engine::Build;
use crate::error::Error;
use crate::internal::EngineShared;
use crate::text::Action;

/// Whether any of `issues` fails a result: an error-severity issue does.
pub(crate) fn fails(issues: &[Diagnostic]) -> bool {
    issues
        .iter()
        .any(|issue| crate::diagnostic::severity(issue) == Severity::Error)
}

/// What one operation produced: its members, in run order, and the
/// project-level issues that refused the whole operation.
pub(crate) struct Outcome {
    pub(crate) members: Vec<Member>,
    pub(crate) issues: Vec<Diagnostic>,
}

/// One member of an outcome: a specification checked or built, or one
/// submission section composed.
pub(crate) struct Member {
    /// The specification's name in its project, when it has one.
    pub(crate) name: Option<String>,
    /// The specification a project operation's issues name as their entry;
    /// absent for an operation on one specification.
    pub(crate) identity: Option<String>,
    pub(crate) work: Work,
}

pub(crate) enum Work {
    Checked(Vec<Diagnostic>),
    Built(Box<Run>),
    Composed(Box<DefineBuild>),
}

impl Member {
    /// The member's issues, each naming its specification as its entry in a
    /// project operation.
    pub(crate) fn issues(&self) -> Vec<Diagnostic> {
        let mut issues = match &self.work {
            Work::Checked(issues) => issues.clone(),
            Work::Built(run) => run.build().issues(),
            Work::Composed(define) => define.issues(),
        };
        if let Some(identity) = &self.identity {
            identify(&mut issues, identity);
        }
        issues
    }

    pub(crate) fn status(&self) -> Status {
        match &self.work {
            Work::Checked(issues) => Status::of_all(issues, []),
            Work::Built(run) => run.status(),
            Work::Composed(define) => Status::of_all(define.composed(), []),
        }
    }

    pub(crate) fn run(&self) -> Option<&Run> {
        match &self.work {
            Work::Built(run) => Some(run),
            _ => None,
        }
    }
}

/// One submission section's composition with its rendered artifacts, a
/// member of a [`crate::Submission`]. Only the save of the submission that
/// holds it writes, through the resolver.
pub(crate) struct DefineBuild {
    inner: yamaa_adapters::file_define::Define,
    /// The issues of the composition as it was made: a later save's refusal
    /// joins its issues, never these.
    composed: Vec<Diagnostic>,
}

impl DefineBuild {
    /// Compose `section` of the project file `project` through the engine's
    /// resolver, with `created` as its creation time when given. The caller
    /// has found that the project declares `section` (PROJECT-14).
    pub(crate) fn compose(
        shared: &EngineShared,
        project: &str,
        section: yamaa_core::project_environment::Submission,
        created: Option<&str>,
        installed: &str,
    ) -> Self {
        let inner = yamaa_adapters::file_define::Define::prepare(
            project,
            section,
            created,
            installed,
            shared.language,
            crate::internal::resolver_bridge(shared),
        );
        let composed = inner.issues().to_vec();
        Self { inner, composed }
    }

    /// The composition's issues, with those of a save that stopped at it.
    pub(crate) fn issues(&self) -> Vec<Diagnostic> {
        self.inner.issues().to_vec()
    }

    /// The composition's issues as it was made, before any save.
    pub(crate) fn composed(&self) -> &[Diagnostic] {
        &self.composed
    }

    /// The transport files of a section that composed: every artifact but
    /// its define.xml, which comes last (JSON-14).
    pub(crate) fn transports(&self) -> usize {
        self.inner.artifacts().len().saturating_sub(1)
    }

    /// The files a save of this composition writes, in publication order:
    /// each transport file, then define.xml (JSON-14). `None` when the
    /// composition, or an earlier save of it, failed.
    pub(crate) fn plan(&self) -> Option<Vec<publication::File<'_>>> {
        self.inner.plan()
    }

    /// Record what a save did with this composition's files; a stop fails the
    /// composition, and its issues then say why.
    pub(crate) fn record(&mut self, committed: usize, stopped: Option<(usize, Cause)>) {
        self.inner.record(committed, stopped);
    }

    /// Record a save stopped at this composition for `issues`, before any of
    /// its files was written.
    pub(crate) fn refuse(&mut self, issues: Vec<Diagnostic>) {
        self.inner.refuse_save(issues);
    }
}

impl Outcome {
    /// The outcome of an operation that a project-level finding refused
    /// before any member.
    pub(crate) fn refused(issues: Vec<Diagnostic>) -> Self {
        Self {
            members: Vec::new(),
            issues,
        }
    }

    /// The project-level issues, then every member's issues in run order.
    pub(crate) fn issues(&self) -> Vec<Diagnostic> {
        let mut issues = self.issues.clone();
        for member in &self.members {
            issues.extend(member.issues());
        }
        issues
    }

    pub(crate) fn status(&self) -> Status {
        Status::of_all(&self.issues, self.members.iter().map(Member::status))
    }

    /// Save every member as one save, whose refusals name `subject`, polling
    /// `token`; see [`save`].
    pub(crate) fn save(
        &mut self,
        operation: Operation,
        subject: &str,
        token: &CancellationToken,
    ) -> Result<Vec<String>, Failure> {
        let parts = self
            .members
            .iter_mut()
            .filter_map(|member| match &mut member.work {
                Work::Built(run) => Some(Part::Built {
                    identity: member.identity.as_deref(),
                    build: &mut run.build,
                }),
                Work::Composed(define) => Some(Part::Composed { define }),
                Work::Checked(_) => None,
            })
            .collect();
        save(&self.issues, parts, operation, subject, token)
    }
}

/// The operation a save belongs to, which says which rule a save with
/// nothing to write breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Run,
    ProjectRun,
    Submission,
}

/// One member as a save sees it.
pub(crate) enum Part<'a> {
    /// A specification's build, with its identity in a project run.
    Built {
        identity: Option<&'a str>,
        build: &'a mut Build,
    },
    /// One submission section's composition.
    Composed { define: &'a mut DefineBuild },
}

impl Part<'_> {
    /// The part's issues, each naming its specification as its entry in a
    /// project run.
    fn issues(&self) -> Vec<Diagnostic> {
        match self {
            Self::Built {
                identity, build, ..
            } => {
                let mut issues = build.issues();
                if let Some(identity) = identity {
                    identify(&mut issues, identity);
                }
                issues
            }
            Self::Composed { define, .. } => define.issues(),
        }
    }

    /// The refusal of a save of `subject` stopped at this part, which wrote
    /// `paths`: the part's issues say why.
    fn refusal(&self, subject: &str, paths: Vec<String>) -> Failure {
        Failure::from_issues(Action::Save(subject), &self.issues()).with_paths(paths)
    }
}

/// One planned part: its position, its files' positions in save order, and
/// what a run's save admitted for them.
struct Planned {
    part: usize,
    files: Range<usize>,
    admitted: Option<Admitted>,
}

/// Save `parts` as one save and return the absolute path of every file
/// written, with forward slashes, in publication order (D38). A caller's
/// resolver names each file by the reference its target reply gave.
///
/// 1. Plan every part's files, in member order: a run's declared report and
///    then its primary artifact (OUTPUT-10), or only its report when any run
///    failed (PROJECT-12); a section's transport files and then its
///    define.xml (JSON-14). A section that did not compose ends the plan: the
///    save stops at it, after the sections before it (PROJECT-13).
/// 2. Open every target, refusing a target that cannot be written and two
///    files that name one target, then stage every file beside its target.
///    Nothing is visible yet, so a save stopped here writes nothing.
/// 3. Poll `token`, once (D74): a cancelled save removes every staged file
///    and fails with the cancellation's error text, writing nothing.
/// 4. Commit the files in order. The first that cannot replace its target
///    stops the save: the files before it stay, and the rest are removed.
///
/// A stop is recorded on its part, whose issues then say why; the refusal
/// carries those issues and the paths written before it (P-API-1). A save
/// that wrote every planned file still refuses when a run failed, with every
/// run's issues, or when a section stopped it, with that section's issues.
/// Every refusal is D39's error text for saving `subject`, the result's
/// subject, whichever member stopped it. A cancelled save records nothing
/// on its parts, so the result can be saved again.
pub(crate) fn save(
    refusal: &[Diagnostic],
    mut parts: Vec<Part<'_>>,
    operation: Operation,
    subject: &str,
    token: &CancellationToken,
) -> Result<Vec<String>, Failure> {
    let action = Action::Save(subject);
    if !refusal.is_empty() {
        return Err(Failure::from_issues(action, refusal));
    }
    if parts.is_empty() {
        let finding = match operation {
            Operation::Submission => yamaa_core::project_scope::submission_nothing_to_save(),
            Operation::Run | Operation::ProjectRun => {
                yamaa_core::project_scope::run_nothing_to_save()
            }
        };
        return Err(Failure::from_finding(action, finding));
    }
    let failed = parts
        .iter()
        .any(|part| matches!(part, Part::Built { build, .. } if build.output().is_none()));

    // 1. Plan every part's files.
    let mut files = Vec::new();
    let mut planned = Vec::new();
    let mut ended = None;
    let mut unplanned = None;
    for (position, part) in parts.iter().enumerate() {
        let start = files.len();
        let admitted = match part {
            Part::Built { build, .. } => match build.plan(failed) {
                Ok(None) => continue,
                Ok(Some((own, admitted))) => {
                    files.extend(own);
                    Some(admitted)
                }
                Err(issues) => {
                    unplanned = Some((position, issues));
                    break;
                }
            },
            Part::Composed { define, .. } => match define.plan() {
                Some(own) => {
                    files.extend(own);
                    None
                }
                None => {
                    ended = Some(position);
                    break;
                }
            },
        };
        planned.push(Planned {
            part: position,
            files: start..files.len(),
            admitted,
        });
    }
    // 2. Open and stage every file.
    let staged = match unplanned {
        // A run whose report cannot be recorded refuses the save unstaged.
        Some(_) => None,
        None => Some(publication::stage(&files)),
    };
    let paths = files
        .iter()
        .map(|file| file.path().to_owned())
        .collect::<Vec<_>>();
    drop(files);
    if let Some((position, issues)) = unplanned {
        if let Part::Built { build, .. } = &mut parts[position] {
            build.refuse(issues);
            build.measure_save("refused");
        }
        return Err(parts[position].refusal(subject, Vec::new()));
    }
    let staged = staged.expect("a planned save stages");
    // 3. D74: poll once, after every file is staged and before the first
    // commit; a cancelled save aborts what it staged and writes nothing.
    if staged.is_ok() && token.is_cancelled() {
        if let Ok(staged) = staged {
            staged.abort();
        }
        return Err(Failure::from_error(action, &Error::Cancelled));
    }
    // 4. Commit in order.
    let committed = staged.and_then(publication::Staged::commit);
    let (written, mut stop) = match committed {
        Ok(written) => (written, None),
        Err(stop) => (paths[..stop.committed].to_vec(), Some(stop)),
    };
    let mut stopped = None;
    for Planned {
        part,
        files,
        admitted,
    } in planned
    {
        let committed = written.len().clamp(files.start, files.end) - files.start;
        let own = stop.take_if(|stop| files.contains(&stop.file));
        if own.is_some() {
            stopped = Some(part);
        }
        // A collision names the earlier file within its own part, or `None`
        // when another part writes it.
        let local = own.map(|stop| {
            let cause = match stop.cause {
                Cause::Collision { earlier } if files.contains(&earlier) => Cause::Collision {
                    earlier: earlier - files.start,
                },
                Cause::Collision { .. } => return (stop.file - files.start, None),
                cause => cause,
            };
            (stop.file - files.start, Some(cause))
        });
        match &mut parts[part] {
            Part::Built { build, .. } => {
                let admitted = admitted.expect("a run's plan admits its observations");
                let local = local.map(|(file, cause)| {
                    (
                        file,
                        cause.unwrap_or(Cause::Collision {
                            earlier: usize::MAX,
                        }),
                    )
                });
                build.measure_save(if local.is_some() || written.len() < files.end {
                    "refused"
                } else if build.output().is_none() {
                    "failed"
                } else {
                    "succeeded"
                });
                build.record(admitted, committed, local);
            }
            Part::Composed { define, .. } => match local {
                Some((_, None)) => define.refuse(publication::shared_target()),
                Some((file, Some(cause))) => define.record(committed, Some((file, cause))),
                None => define.record(committed, None),
            },
        }
    }
    if let Some(part) = stopped.or(ended) {
        return Err(parts[part].refusal(subject, written));
    }
    if failed {
        let issues = parts.iter().flat_map(Part::issues).collect::<Vec<_>>();
        return Err(Failure::from_issues(action, &issues).with_paths(written));
    }
    Ok(written)
}
