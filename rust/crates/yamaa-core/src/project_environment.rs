//! Typed static environment admission without code, lock-version or study ports.
use crate::{
    ast::{self, Surface},
    project_function::{self, Language},
    project_terminology,
};
use std::collections::BTreeSet;

/// Format is obtained from captured lock syntax, never inferred from a basename.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockKind {
    Uv,
    Renv,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LockReference {
    pub written: String,
    pub kind: LockKind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Submission {
    Sdtm,
    Adam,
    Send,
}

/// The definitions and codelist sources lowered from a project's files, in
/// declaration order. Study and section metadata stay with the project root;
/// only submission composition requires them.
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    /// The surface the project file is written in. A 0.5 lock is declared by
    /// the linked environment file, and a 1.0 lock by the project file (D8).
    pub surface: Surface,
    /// The `environments` key the project links for the host. Absent when a
    /// 0.5 project names no environment file for the host language, and for
    /// an engine that claims no host language, which reads no `environments`
    /// entry (#3030 section 2.10); a 1.0 project's is otherwise always the
    /// host's, since one host language runs a whole build and its function
    /// files are language-neutral (D8).
    pub language: Option<Language>,
    pub lock: Option<LockReference>,
    pub functions: Option<Vec<ast::Function>>,
    pub codelists: Vec<ast::Codelists>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    LockKindMismatch {
        language: Language,
        actual: LockKind,
    },
    DuplicateFunction {
        function: usize,
    },
    Function {
        function: usize,
        finding: project_function::Finding,
    },
    Codelist(project_terminology::Finding),
}

/// Retain the original typed input for diagnostic projection after rejection.
/// The boxed draft transfers ownership without copying functions or terminology.
#[derive(Debug)]
pub struct RejectedDraft {
    pub draft: Box<Draft>,
    pub error: Vec<Finding>,
}

/// The immutable snapshot remains a static result; it grants no activation.
/// `host` is the host language, or none for an engine that claims no host
/// language, such as the command line's (#3030 section 2.10).
#[derive(Clone, Debug, PartialEq)]
pub struct Environment {
    host: Option<Language>,
    draft: Draft,
}

/// Why a supplied project declares no function a call could name, when the
/// project's environment rather than the call's name is the cause (FUNC-1,
/// PROJECT-2). A 0.5 project declares its functions in the environment file
/// each host language's key links; a 1.0 project lists language-neutral
/// function files that every host reads (D8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Undeclared {
    /// The 0.5 project links no environment file for this host language.
    Language(Language),
    /// The engine claims no host language, so it reads no 0.5 environment
    /// file (#3030 section 2.10, D123).
    NoHostLanguage,
}

/// One immutable execution model moved from static admission. Callable tests and
/// terminology values remain owned once, independently of the number of calls.
#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionEnvironment {
    host: Option<Language>,
    language: Option<Language>,
    lock: Option<LockReference>,
    functions: Option<Vec<project_function::Function>>,
    catalogue: project_terminology::Catalogue,
    inventory: crate::submission_inventory::Inventory,
    /// False only for a build that supplied no project file at all.
    project: bool,
    /// A supplied 0.5 project that links no environment file for the host,
    /// or for an engine that claims no host language.
    undeclared: bool,
}
impl ExecutionEnvironment {
    /// A build without a project has no environment, terminology or inventory.
    /// It does not manufacture a project file or a discovery location.
    pub fn without_project(host: Option<Language>) -> Self {
        Self {
            host,
            language: None,
            lock: None,
            functions: None,
            catalogue: project_terminology::Catalogue::from_admitted(Vec::new()),
            inventory: Default::default(),
            project: false,
            undeclared: false,
        }
    }
    /// The host language, or none for an engine that claims no host
    /// language, which reads no `environments` entry (#3030 section 2.10).
    pub fn host(&self) -> Option<Language> {
        self.host
    }
    pub fn language(&self) -> Option<Language> {
        self.language
    }
    /// Whether a project file was supplied. A build without one has no
    /// project metadata: no codelists, inventory or functions bind.
    pub fn is_project(&self) -> bool {
        self.project
    }
    /// Why the supplied 0.5 project declares no function for the host, if it
    /// declares none: it links no environment file for the host language, or
    /// the engine claims no host language. A specification that calls a
    /// project function cannot execute then.
    pub fn undeclared(&self) -> Option<Undeclared> {
        self.undeclared.then(|| {
            self.host
                .map_or(Undeclared::NoHostLanguage, Undeclared::Language)
        })
    }
    pub fn lock(&self) -> Option<&LockReference> {
        self.lock.as_ref()
    }
    pub fn functions_present(&self) -> bool {
        self.functions.is_some()
    }
    pub fn functions(&self) -> &[project_function::Function] {
        self.functions.as_deref().unwrap_or(&[])
    }
    pub fn catalogue(&self) -> &project_terminology::Catalogue {
        &self.catalogue
    }
    pub fn inventory(&self) -> &crate::submission_inventory::Inventory {
        &self.inventory
    }
    pub fn attach_inventory(&mut self, inventory: crate::submission_inventory::Inventory) {
        self.inventory = inventory;
    }
}
impl Environment {
    pub fn admit(host: Option<Language>, draft: Draft) -> Result<Self, Vec<Finding>> {
        Self::admit_retained(host, draft).map_err(|rejected| rejected.error)
    }
    /// Admit once while retaining the original typed ownership on either outcome.
    pub fn admit_retained(host: Option<Language>, draft: Draft) -> Result<Self, RejectedDraft> {
        let findings = validate(host, &draft);
        if findings.is_empty() {
            Ok(Self { host, draft })
        } else {
            Err(RejectedDraft {
                draft: Box::new(draft),
                error: findings,
            })
        }
    }
    /// Keep admitted typed data when another source prevents aggregate success.
    pub fn into_draft(self) -> Draft {
        self.draft
    }
    pub fn draft(&self) -> &Draft {
        &self.draft
    }
    /// Static admission is the only constructor of this retained proof. Moving
    /// its definitions preserves the checked aggregate policy and exact bytes;
    /// this conversion grants neither code execution nor study authority.
    pub fn into_execution(self) -> ExecutionEnvironment {
        let Draft {
            surface,
            language,
            lock,
            functions,
            codelists,
        } = self.draft;
        ExecutionEnvironment {
            host: self.host,
            language,
            undeclared: surface == Surface::V0_5 && language.is_none(),
            lock,
            functions: functions.map(|definitions| {
                definitions
                    .into_iter()
                    .map(|definition| {
                        project_function::Function::from_admitted(self.host, definition)
                    })
                    .collect()
            }),
            catalogue: project_terminology::Catalogue::from_admitted(codelists),
            inventory: Default::default(),
            project: true,
        }
    }
}

pub fn validate(host: Option<Language>, draft: &Draft) -> Vec<Finding> {
    let mut findings = Vec::new();
    // The project key states the environment's language; only its lock format
    // can still disagree with it.
    if let (Some(language), Some(lock)) = (draft.language, &draft.lock) {
        if !matches!(
            (language, lock.kind),
            (Language::Python, LockKind::Uv) | (Language::R, LockKind::Renv)
        ) {
            findings.push(Finding::LockKindMismatch {
                language,
                actual: lock.kind,
            });
        }
    }
    if let Some(functions) = &draft.functions {
        let mut names = BTreeSet::new();
        for (function, definition) in functions.iter().enumerate() {
            if !names.insert(&definition.name.name) {
                findings.push(Finding::DuplicateFunction { function });
            }
            for finding in project_function::validate(draft.language.or(host), definition) {
                findings.push(Finding::Function { function, finding });
            }
        }
    }
    findings.extend(
        project_terminology::validate(&draft.codelists)
            .into_iter()
            .map(Finding::Codelist),
    );
    findings
}
