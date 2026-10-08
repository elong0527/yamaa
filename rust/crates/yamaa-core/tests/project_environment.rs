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
