#[path = "support/project_codelist_compiler.rs"]
mod support;
use support::{environment, schema, spec};
use yamaa_core::{
    dataset::Check,
    specification::{PrepareError, PreparedSpecification},
    value::Value,
};
#[test]
fn fixed_column_checkpoint_is_compiled_for_ordinary_and_row_plans_without_activation() {
    for rows in [false, true] {
        let doc = spec("SEX", "str", None, rows, None);
        let env = environment(false);
        let prepared = PreparedSpecification::prepare_with_environment(&doc, &env).unwrap();
        assert!(prepared.called_functions().is_empty());
        assert!(prepared.verification_declaration_diagnostics().is_empty());
        assert_eq!(
            prepared.verification_target("columns.CODE.submission.codelist"),
            Some("CODE")
        );
        assert_eq!(
            prepared.verification_identity("columns.CODE.submission.codelist"),
            None
        );
        let plan = prepared.bind(&schema()).unwrap();
        assert_eq!(plan.column_verifications().len(), 1);
        assert_eq!(plan.column_verifications()[0].column, 1);
        assert!(
            matches!(&plan.column_verifications()[0].checks[0].check,Check::Codelist {id,values} if id=="SEX" && values==&[Value::Str("F".into())])
        );
    }
}
#[test]
fn compiler_projects_unknown_type_and_conflicting_sets_as_static_preflight_causes() {
    for (doc, condition, requirement) in [
        (
            spec("UNKNOWN", "str", None, false, None),
            "unknown_codelist",
            "REQ-0953",
        ),
        (
            spec("SEX", "int", None, false, None),
            "codelist_type_mismatch",
            "REQ-0954",
        ),
        (
            spec("SEX", "str", Some("M"), false, None),
            "codelist_values_conflict",
            "REQ-0955",
        ),
    ] {
        let Err(PrepareError::Invalid(findings)) =
            PreparedSpecification::prepare_with_environment(&doc, &environment(false))
        else {
            panic!("static rejection")
        };
        assert_eq!(findings.len(), 1);
        let d = findings[0].diagnostic();
        assert_eq!(
            (
                d.definition().phase,
                d.definition().condition,
                d.definition().requirement
            ),
            ("validation", condition, Some(requirement))
        );
        assert_eq!(d.spec_paths[0], "columns.CODE.submission.codelist");
    }
}
#[test]
fn extensible_catalogue_does_not_replace_the_ordinary_allowed_values_check() {
    let doc = spec("SEX", "str", Some("M"), false, None);
    let prepared =
        PreparedSpecification::prepare_with_environment(&doc, &environment(true)).unwrap();
    let plan = prepared.bind(&schema()).unwrap();
    assert_eq!(plan.column_verifications()[0].checks.len(), 1);
    assert!(
        matches!(&plan.column_verifications()[0].checks[0].check,Check::AllowedValues(values) if values==&[Value::Str("M".into())])
    );
}
#[test]
fn explicit_catalogue_does_not_open_other_submission_or_reserved_metadata_routes() {
    for rows in [false, true] {
        let doc = spec("SEX", "str", None, rows, None);
        assert!(matches!(
            PreparedSpecification::prepare(&doc),
            Err(PrepareError::Unsupported(_))
        ));
        for extra in ["role", "metadata"] {
            assert!(matches!(
                PreparedSpecification::prepare_with_environment(
                    &spec("SEX", "str", None, rows, Some(extra)),
                    &environment(false)
                ),
                Err(PrepareError::Unsupported(_))
            ));
        }
    }
}
#[test]
fn compiler_terminology_quota_refuses_the_entire_environment_plan() {
    let doc = spec("SEX", "str", None, false, None);
    assert!(matches!(
        PreparedSpecification::prepare_with_environment_limits(
            &doc,
            &environment(false),
            Default::default(),
            yamaa_core::project_limits::Limits {
                text_bytes: 1,
                ..Default::default()
            }
        ),
        Err(PrepareError::Limit("terminology_bindings"))
    ));
}
