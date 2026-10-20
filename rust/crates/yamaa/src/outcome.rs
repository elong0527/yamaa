//! One operation's outcome: its members, in run order, and the project-level
//! issues that refused the whole operation. A specification run is a graph of
//! one member; a project run, check or submission has one member per
//! specification or section. The public views in [`crate::api`] read it, and
//! each computes its status from the issues and the work; nothing here stores
//! a status.

use yamaa_adapters::project_scope::identify;
use yamaa_core::diagnostic::Diagnostic;
use yamaa_core::project_environment::Submission as Section;

use crate::api::{Run, Status};
use crate::diagnostic::Severity;
use crate::engine::DefineBuild;

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
    Composed(Section, Box<DefineBuild>),
}

impl Member {
    /// The member's issues, each naming its specification as its entry in a
    /// project operation.
    pub(crate) fn issues(&self) -> Vec<Diagnostic> {
        let mut issues = match &self.work {
            Work::Checked(issues) => issues.clone(),
            Work::Built(run) => run.build().issues(),
            Work::Composed(_, define) => define.issues(),
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
            Work::Composed(_, define) => Status::of_all(define.composed(), []),
        }
    }

    pub(crate) fn run(&self) -> Option<&Run> {
        match &self.work {
            Work::Built(run) => Some(run),
            _ => None,
        }
    }

    /// The name a message uses for the member: its name, else its identity.
    pub(crate) fn label(&self) -> &str {
        self.name
            .as_deref()
            .or(self.identity.as_deref())
            .unwrap_or_default()
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
}
