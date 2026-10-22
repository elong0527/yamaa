//! `str_replace` execution contracts.
use crate::support;

#[test]
fn oversized_patterns_report_the_compiled_size_limit_at_the_pattern() {
    use support::document::Tree as T;
    use yamaa_core::{
        limits::Limits,
        schema::SpecificationDocument,
        specification::{CompilerLimit, PrepareError, PreparedSpecification},
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
            ("schema_version", T::text("1.0")),
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
        let model = SpecificationDocument::admit(document).unwrap().unwrap();
        let Err(PrepareError::Limit(limit)) = PreparedSpecification::prepare(&model) else {
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
