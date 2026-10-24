#![cfg(feature = "yaml")]
//! Text pad: refusal before a domain is returned, which the diagnostic
//! replay cannot distinguish from a built domain reporting the same issues.
//! The authored inputs and issue rows stay in `fixtures/diagnostics/text_pad.json`.

#[test]
fn an_unrepresentable_width_refuses_the_run_before_returning_a_domain() {
    use crate::support::{diagnostic, study};

    let case = diagnostic::replay::cases()
        .into_iter()
        .find(|case| {
            case.label
                == "text_pad.json::a_present_value_refuses_the_whole_output_at_an_unrepresentable_width"
        })
        .expect("the authored present-value padding case");
    let directory = diagnostic::operations::study(&case.input);
    let specification = directory.join(case.input["specification"].as_str().unwrap());
    let specification = specification.to_str().unwrap();
    assert!(
        study::domain(
            yamaa_core::project_function::Language::Python,
            yamaa_core::domain_entry::Request {
                specification,
                environment: None,
            },
            study::identity("str-pad", specification),
        )
        .is_err(),
        "{}: a present value cannot be padded to the width",
        case.label
    );
}
