//! The partitions every client runs over (#3794, D264, #4193): the Python
//! and R bindings and the command line each build their engine over
//! [`yamaa::Partitions::client_default`], with the threads their own
//! argument gives.

/// A host's `threads` argument (#4193, D-next on #4184) is a positive whole
/// number, or missing for the default; a number beyond what the platform
/// counts saturates. Only R gives a whole number as a double; Python's is an
/// `int` (`bindings.yaml`, `types.threads`, #5080). The seam is the hosts'
/// one judgement, the host-internal `threads`, which the Python and R
/// bindings each reach with their own spelling of the value.
#[test]
fn a_host_threads_argument_is_a_positive_whole_number_or_missing() {
    use std::num::NonZeroUsize;
    use yamaa::host_internal::threads as admit;
    use yamaa::{ContextValue as V, HostLanguage, Partitions};
    let base = Partitions::new(NonZeroUsize::MIN);
    let threads = |count: usize| Some(base.with_threads(NonZeroUsize::new(count).unwrap()));
    // Each case: its label, the value, and what Python and R admit.
    let cases = [
        ("missing", V::Null, Some(base), Some(base)),
        ("one", V::Int(1), threads(1), threads(1)),
        ("four", V::Int(4), threads(4), threads(4)),
        ("a whole double", V::Float(2.0), None, threads(2)),
        (
            "an integer beyond 64 bits",
            V::Wide("18446744073709551616".into()),
            threads(usize::MAX),
            threads(usize::MAX),
        ),
        ("zero", V::Int(0), None, None),
        ("negative", V::Int(-1), None, None),
        ("a zero double", V::Float(0.0), None, None),
        ("a negative double", V::Float(-2.0), None, None),
        ("a fraction", V::Float(1.5), None, None),
        ("a double beyond 2^53", V::Float(2f64.powi(60)), None, None),
        ("not a number", V::Float(f64::NAN), None, None),
        ("infinite", V::Float(f64::INFINITY), None, None),
        (
            "a negative integer beyond 64 bits",
            V::Wide("-18446744073709551616".into()),
            None,
            None,
        ),
        ("a Boolean", V::Bool(true), None, None),
        ("text", V::Text("4".into()), None, None),
    ];
    for (label, value, python, r) in cases {
        assert_eq!(
            (
                admit(HostLanguage::Python, base, &value),
                admit(HostLanguage::R, base, &value)
            ),
            (python, r),
            "{label}"
        );
    }
}
