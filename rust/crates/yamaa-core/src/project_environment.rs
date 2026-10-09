//! Typed static environment admission without code, lock-version or study ports.
use crate::project_limits::{AdmissionError, Budget, Limits};
use crate::{
    project_function::{self, Definition, Language},
    project_terminology::{self, Source},
};
use alloc::{boxed::Box, collections::BTreeSet, string::String, vec::Vec};

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

/// Closed classes and required fields have already passed shared schema admission.
/// Retained submission metadata stays with the captured normalized document.
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub language: Option<Language>,
    pub lock: Option<LockReference>,
    pub functions: Option<Vec<Definition>>,
    pub codelists: Vec<Source>,
    pub has_study: bool,
    pub submissions: Vec<Submission>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finding {
    MissingLanguage,
    MissingLock,
    LanguageMismatch {
        declared: Language,
        host: Language,
    },
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
    MissingStudy,
}

/// Retain the original typed input for diagnostic projection after rejection.
/// The boxed draft transfers ownership without copying functions or terminology.
#[derive(Debug)]
pub struct RejectedDraft {
    pub draft: Box<Draft>,
    pub error: AdmissionError<Finding>,
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
    has_study: bool,
    submissions: Vec<Submission>,
}
impl ExecutionEnvironment {
    pub fn host(&self) -> Language {
        self.host
    }
    pub fn language(&self) -> Option<Language> {
        self.language
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
    pub fn has_study(&self) -> bool {
        self.has_study
    }
    pub fn submissions(&self) -> &[Submission] {
        &self.submissions
    }
}
impl Environment {
    pub fn admit(host: Language, draft: Draft) -> Result<Self, AdmissionError<Finding>> {
        Self::admit_with_limits(host, draft, Limits::default())
    }
    pub fn admit_with_limits(
        host: Language,
        draft: Draft,
        limits: Limits,
    ) -> Result<Self, AdmissionError<Finding>> {
        Self::admit_retained_with_limits(host, draft, limits).map_err(|rejected| rejected.error)
    }
    /// Admit once while retaining the original typed ownership on either outcome.
    pub fn admit_retained_with_limits(
        host: Language,
        draft: Draft,
        limits: Limits,
    ) -> Result<Self, RejectedDraft> {
        let admission = (|| {
            let mut budget = Budget::new(limits);
            budget.findings(6).map_err(AdmissionError::Limit)?;
            if let Some(lock) = &draft.lock {
                budget.text(&lock.written).map_err(AdmissionError::Limit)?;
            }
            if let Some(functions) = &draft.functions {
                budget.functions(functions).map_err(AdmissionError::Limit)?;
            }
            budget
                .sources(&draft.codelists)
                .map_err(AdmissionError::Limit)?;
            let findings = validate(host, &draft);
            if findings.is_empty() {
                Ok(())
            } else {
                Err(AdmissionError::Findings(findings))
            }
        })();
        match admission {
            Ok(()) => Ok(Self { host, draft }),
            Err(error) => Err(RejectedDraft {
                draft: Box::new(draft),
                error,
            }),
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
            has_study,
            submissions,
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
            has_study,
            submissions,
        }
    }
}

pub fn validate(host: Language, draft: &Draft) -> Vec<Finding> {
    let mut findings = Vec::new();
    if draft.functions.is_some() {
        if draft.language.is_none() {
            findings.push(Finding::MissingLanguage);
        }
        if draft.lock.is_none() {
            findings.push(Finding::MissingLock);
        }
    }
    if let Some(language) = draft.language {
        if language != host {
            findings.push(Finding::LanguageMismatch {
                declared: language,
                host,
            });
        }
        if let Some(lock) = &draft.lock {
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
    }
    if let Some(functions) = &draft.functions {
        let mut names = BTreeSet::new();
        for (function, definition) in functions.iter().enumerate() {
            if !names.insert(&definition.name) {
                findings.push(Finding::DuplicateFunction { function });
            }
            // The selected host gives useful callable findings even when language
            // is absent; absence itself remains an independent root finding.
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
    if !draft.submissions.is_empty() && !draft.has_study {
        findings.push(Finding::MissingStudy);
    }
    findings
}
