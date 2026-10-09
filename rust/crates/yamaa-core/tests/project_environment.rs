use yamaa_core::{
    project_environment::{
        validate, Draft, Environment, Finding, LockKind, LockReference, Submission,
    },
    project_function::{Case, Definition, Language},
    value::{ColumnType, Value},
};
fn draft() -> Draft {
    Draft {
        language: None,
        lock: None,
        functions: None,
        codelists: vec![],
        has_study: false,
        submissions: vec![],
    }
}
fn definition() -> Definition {
    Definition {
        name: "constant".into(),
        function: "project.constant".into(),
        description: "Constant integer.".into(),
        params: vec![],
        returns: ColumnType::Int,
        may_return_missing: false,
        comparison_decimals: 4,
        tests: vec![Case {
            id: "ordinary".into(),
            covers: vec!["normal".into(), "boundary".into()],
            args: vec![],
            result: Value::Int(1),
        }],
    }
}
#[test]
fn empty_environment_and_submission_only_environments_need_no_code_or_lock() {
    assert!(Environment::admit(Language::Python, draft()).is_ok());
    let mut submission = draft();
    submission.has_study = true;
    submission.submissions = vec![Submission::Sdtm, Submission::Adam, Submission::Send];
    assert!(Environment::admit(Language::R, submission).is_ok());
}
#[test]
fn present_functions_even_empty_require_language_and_lock() {
    let mut d = draft();
    d.functions = Some(vec![]);
    assert_eq!(
        validate(Language::Python, &d),
        vec![Finding::MissingLanguage, Finding::MissingLock]
    );
    d.functions = None;
    d.language = Some(Language::Python);
    assert!(validate(Language::Python, &d).is_empty());
}
#[test]
fn declared_language_and_captured_lock_kind_are_independent_findings() {
    let mut d = draft();
    d.language = Some(Language::R);
    d.lock = Some(LockReference {
        written: "custom-name.lock".into(),
        kind: LockKind::Uv,
    });
    assert_eq!(
        validate(Language::Python, &d),
        vec![
            Finding::LanguageMismatch {
                declared: Language::R,
                host: Language::Python
            },
            Finding::LockKindMismatch {
                language: Language::R,
                actual: LockKind::Uv
            }
        ]
    );
    d.language = Some(Language::Python);
    assert!(validate(Language::Python, &d).is_empty());
}
#[test]
fn all_function_and_root_faults_remain_owned_without_any_effect_port() {
    let mut d = draft();
    d.functions = Some(vec![definition(), definition()]);
    d.submissions = vec![Submission::Adam];
    assert_eq!(
        validate(Language::Python, &d),
        vec![
            Finding::MissingLanguage,
            Finding::MissingLock,
            Finding::DuplicateFunction { function: 1 },
            Finding::MissingStudy
        ]
    );
    d.language = Some(Language::Python);
    d.lock = Some(LockReference {
        written: "../uv.lock".into(),
        kind: LockKind::Uv,
    });
    d.functions.as_mut().unwrap().pop();
    d.has_study = true;
    let expected = d.clone();
    let admitted = Environment::admit(Language::Python, d).unwrap();
    assert_eq!(admitted.draft(), &expected);
}

#[test]
fn execution_conversion_moves_exact_definitions_and_preserves_absent_or_empty_functions() {
    let empty = Environment::admit(Language::R, draft())
        .unwrap()
        .into_execution();
    assert_eq!(empty.host(), Language::R);
    assert_eq!(empty.language(), None);
    assert!(!empty.functions_present());
    assert!(empty.functions().is_empty());
    assert!(empty.lock().is_none());
    let mut d = draft();
    d.language = Some(Language::Python);
    d.lock = Some(LockReference {
        written: "uv.lock".into(),
        kind: LockKind::Uv,
    });
    d.functions = Some(vec![]);
    let empty = Environment::admit(Language::Python, d.clone())
        .unwrap()
        .into_execution();
    assert!(empty.functions_present());
    assert!(empty.functions().is_empty());
    d.functions = Some(vec![definition()]);
    d.has_study = true;
    d.submissions = vec![Submission::Adam];
    let expected = d.clone();
    let original_name = d.functions.as_ref().unwrap()[0].name.as_ptr();
    let original_cases = d.functions.as_ref().unwrap()[0].tests.as_ptr();
    let execution = Environment::admit(Language::Python, d)
        .unwrap()
        .into_execution();
    assert_eq!(execution.language(), Some(Language::Python));
    assert_eq!(execution.lock(), expected.lock.as_ref());
    assert_eq!(
        execution.functions()[0].definition(),
        &expected.functions.as_ref().unwrap()[0]
    );
    assert_eq!(
        execution.functions()[0].definition().name.as_ptr(),
        original_name
    );
    assert_eq!(
        execution.functions()[0].definition().tests.as_ptr(),
        original_cases
    );
    assert_eq!(execution.functions()[0].language(), Language::Python);
    assert!(execution.has_study());
    assert_eq!(execution.submissions(), [Submission::Adam]);
    assert!(execution.catalogue().sources().is_empty());
}

#[test]
fn rejected_admission_retains_exact_draft_ownership_for_findings_and_limits() {
    use yamaa_core::project_limits::{AdmissionError, Limit, Limits, Resource};
    let mut input = draft();
    input.functions = Some(vec![definition(), definition()]);
    input.submissions = vec![Submission::Adam];
    let expected = input.clone();
    let functions = input.functions.as_ref().unwrap().as_ptr();
    let cases = input.functions.as_ref().unwrap()[0].tests.as_ptr();
    let rejected =
        Environment::admit_retained_with_limits(Language::Python, input, Limits::default())
            .unwrap_err();
    assert_eq!(*rejected.draft, expected);
    assert_eq!(
        rejected.draft.functions.as_ref().unwrap().as_ptr(),
        functions
    );
    assert_eq!(
        rejected.draft.functions.as_ref().unwrap()[0].tests.as_ptr(),
        cases
    );
    assert_eq!(
        rejected.error,
        AdmissionError::Findings(vec![
            Finding::MissingLanguage,
            Finding::MissingLock,
            Finding::DuplicateFunction { function: 1 },
            Finding::MissingStudy
        ])
    );
    let input = *rejected.draft;
    let rejected = Environment::admit_retained_with_limits(
        Language::Python,
        input,
        Limits {
            text_bytes: 0,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(*rejected.draft, expected);
    assert_eq!(
        rejected.draft.functions.as_ref().unwrap().as_ptr(),
        functions
    );
    assert_eq!(
        rejected.draft.functions.as_ref().unwrap()[0].tests.as_ptr(),
        cases
    );
    assert_eq!(
        rejected.error,
        AdmissionError::Limit(Limit {
            resource: Resource::TextBytes
        })
    );
}
