//! What a host's values hold exactly: the host-internal `unheld`, the one
//! judgement the R binding applies to a project function's argument and a
//! table's cell, and the command line's R worker to an argument before it
//! sends the call.

/// FUNC-9's R adapter: an `int` beyond 2^53 in magnitude or text containing
/// NUL has no exact R value; Python holds every value. The seam is the
/// judgement itself, which the R suites and the command line's stand-in
/// worker reach only through a run.
#[test]
fn r_holds_ints_within_2_53_and_text_without_nul_and_python_holds_every_value() {
    use yamaa::host_internal::unheld;
    use yamaa::scalar::ScalarRef;
    use yamaa::HostLanguage;
    let beyond = |value: i64| {
        Some(format!(
            "integer {value} is beyond 2^53 and cannot be held exactly by an R double"
        ))
    };
    let nul = Some("text containing NUL cannot be held by an R character vector".to_owned());
    let cases = [
        (ScalarRef::Int(1 << 53), None),
        (ScalarRef::Int(-(1 << 53)), None),
        (ScalarRef::Int((1 << 53) + 1), beyond((1 << 53) + 1)),
        (ScalarRef::Int(-(1 << 53) - 1), beyond(-(1 << 53) - 1)),
        (ScalarRef::Int(i64::MIN), beyond(i64::MIN)),
        (ScalarRef::Str("a\0b"), nul),
        (ScalarRef::Str("ab"), None),
        (ScalarRef::Float(1e300), None),
    ];
    for (value, reason) in cases {
        assert_eq!(unheld(HostLanguage::R, value), reason, "R {value:?}");
        assert_eq!(
            unheld(HostLanguage::Python, value),
            None,
            "Python {value:?}"
        );
    }
}
