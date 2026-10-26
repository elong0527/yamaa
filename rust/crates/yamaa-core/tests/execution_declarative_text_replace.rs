//! `str_replace` execution contracts.
//!
//! The seam is `prepare`'s typed `PrepareError::Limit` and its
//! `CompilerLimit`, which no run reports; the host-fixture case
//! `pattern_beyond_the_compiled_size_limit` proves the public report.
use crate::support;

/// No benchmark compiles a pattern beyond the size limit. This proves the
/// compiler's typed refusal for both operations; the host-fixture case
/// `pattern_beyond_the_compiled_size_limit` proves its public report.
#[test]
fn oversized_patterns_report_the_compiled_size_limit_at_the_pattern() {
    use support::document::Tree as T;
    use support::specification::prepare;
    use yamaa_core::{
        limits::Limits,
        specification::{CompilerLimit, PrepareError},
    };

    // A valid repetition count far beyond any compiled program the limit admits.
    let pattern = format!("(?:ab){{{}}}", usize::MAX);
    for operation in ["str_extract", "str_replace"] {
        let mut fields = vec![
            ("source", T::text("SRC.TEXT")),
            ("pattern", T::text(&pattern)),
        ];
        if operation == "str_replace" {
            fields.push(("replacement", T::text("-")));
        }
        let document = T::map(vec![
            ("yamaa", T::text("1.0")),
            ("type", T::text("spec")),
            ("domain", T::text("TEST")),
            ("base", T::text("SRC")),
            (
                "input",
                T::map(vec![("SRC", T::map(vec![("path", T::text("input.csv"))]))]),
            ),
            ("keys", T::list(vec![T::text("TEXT")])),
            (
                "columns",
                T::list(vec![T::map(vec![
                    ("name", T::text("TEXT")),
                    ("type", T::text("str")),
                    (
                        "derivation",
                        T::map(vec![("value", T::map(vec![(operation, T::map(fields))]))]),
                    ),
                ])]),
            ),
            (
                "output",
                T::map(vec![
                    ("path", T::text("output.csv")),
                    ("columns", T::list(vec![T::text("TEXT")])),
                ]),
            ),
        ])
        .document();
        let Err(PrepareError::Limit(limit)) = prepare(&document) else {
            panic!("an oversized pattern must be refused for {operation}");
        };
        assert_eq!(
            limit,
            CompilerLimit {
                resource: "regex_size",
                limit: Limits::REGEX_SIZE,
                actual: None,
                spec_paths: vec![format!("columns.TEXT.derivation.{operation}.pattern")],
            }
        );
    }
}
