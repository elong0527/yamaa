//! The partitions every client runs over (#3794, D264, #4193): the Python
//! and R bindings and the command line each build their engine over
//! [`yamaa::Partitions::client_default`], with the threads their own
//! argument gives.

/// A host's `threads` argument (#4193, D-next on #4184) is a positive whole
/// number, which R may give as a whole double, or missing for the default;
/// a number beyond what the platform counts saturates. The seam is the
/// facade's one judgement, which the Python and R bindings and the command
/// line each reach with their own spelling of the value.
#[test]
fn a_host_threads_argument_is_a_positive_whole_number_or_missing() {
    use std::num::NonZeroUsize;
    use yamaa::{ContextValue as V, Partitions};
    let base = Partitions::new(NonZeroUsize::MIN);
    let threads = |count: usize| Some(base.with_threads(NonZeroUsize::new(count).unwrap()));
    let cases = [
        ("missing", V::Null, Some(base)),
        ("one", V::Int(1), threads(1)),
        ("four", V::Int(4), threads(4)),
        ("a whole double", V::Float(2.0), threads(2)),
        (
            "an integer beyond 64 bits",
            V::Wide("18446744073709551616".into()),
            threads(usize::MAX),
        ),
        ("zero", V::Int(0), None),
        ("negative", V::Int(-1), None),
        ("a zero double", V::Float(0.0), None),
        ("a fraction", V::Float(1.5), None),
        ("not a number", V::Float(f64::NAN), None),
        ("infinite", V::Float(f64::INFINITY), None),
        (
            "a negative integer beyond 64 bits",
            V::Wide("-18446744073709551616".into()),
            None,
        ),
        ("a Boolean", V::Bool(true), None),
        ("text", V::Text("4".into()), None),
    ];
    for (label, value, expected) in cases {
        assert_eq!(base.with_host_threads(&value), expected, "{label}");
    }
}
