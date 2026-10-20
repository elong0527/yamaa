//! Typed static environment admission without code, lock-version or study ports.
use crate::{
    ast,
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
    /// The `environments` key the project links for the host. Absent when the
    /// project names no environment file for the host language.
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
#[derive(Clone, Debug, PartialEq)]
pub struct Environment {
    host: Language,
    draft: Draft,
}

/// One immutable execution model moved from static admission. Callable tests and
/// terminology values remain owned once, independently of the number of calls.
#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionEnvironment {
    host: Language,
    language: Option<Language>,
    lock: Option<LockReference>,
    functions: Option<Vec<project_function::Function>>,
    catalogue: project_terminology::Catalogue,
    inventory: crate::submission_inventory::Inventory,
    /// False only for a build that supplied no project file at all.
    project: bool,
}
impl ExecutionEnvironment {
    /// A build without a project has no environment, terminology or inventory.
    /// It does not manufacture a project file or a discovery location.
    pub fn without_project(host: Language) -> Self {
        Self {
            host,
            language: None,
            lock: None,
            functions: None,
            catalogue: project_terminology::Catalogue::from_admitted(Vec::new()),
            inventory: Default::default(),
            project: false,
        }
    }
    pub fn host(&self) -> Language {
        self.host
    }
    pub fn language(&self) -> Option<Language> {
        self.language
    }
    /// The host language when a supplied project links no environment for it.
    /// A specification that calls a project function cannot execute then.
    pub fn undeclared_language(&self) -> Option<Language> {
        (self.project && self.language.is_none()).then_some(self.host)
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
    pub fn admit(host: Language, draft: Draft) -> Result<Self, Vec<Finding>> {
        Self::admit_retained(host, draft).map_err(|rejected| rejected.error)
    }
    /// Admit once while retaining the original typed ownership on either outcome.
    pub fn admit_retained(host: Language, draft: Draft) -> Result<Self, RejectedDraft> {
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
            language,
            lock,
            functions,
            codelists,
        } = self.draft;
        ExecutionEnvironment {
            host: self.host,
            language,
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

pub fn validate(host: Language, draft: &Draft) -> Vec<Finding> {
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
            for finding in project_function::validate(draft.language.unwrap_or(host), definition) {
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
