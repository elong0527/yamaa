#![cfg(feature = "yaml")]
//! Declaration contracts read from their authored truth: the grammar, regex
//! and rounding vectors and the syntax controls, the ODM declaration
//! projection's catalog and the original invalid intermediate verifications.
//! The ODM declaration refusals and the other original intermediate
//! verifications are cases of `fixtures/diagnostics/declaration_vectors.json`,
//! which the replay reads.
mod declaration {
    mod contract_vectors {
        //! Direct native checks of unchanged authored grammar, regex and rounding truth.
        use std::collections::BTreeSet;
        use yamaa_adapters::json;
        use yamaa_adapters::yaml_decode::decode_yaml;
        use yamaa_core::expression::{
            compile, Comparison, CompileError, CompiledExpression, Context, Failure, NodeKind,
            TemporalKind, RESERVED,
        };
        use yamaa_core::json::Value as Json;
        use yamaa_core::numeric::{BinaryOperator, UnaryOperator};
        use yamaa_core::schema::{Document, DocumentNode};
        use yamaa_core::text_template::{Part, StringTemplate};

        fn quote(text: &str) -> String {
            format!("'{}'", text.replace('\'', "''"))
        }
        fn binary(op: BinaryOperator) -> &'static str {
            match op {
                BinaryOperator::Add => "+",
                BinaryOperator::Subtract => "-",
                BinaryOperator::Multiply => "*",
                BinaryOperator::Divide => "/",
                BinaryOperator::Modulo => unreachable!("MOD is a call"),
            }
        }
        fn unary(op: UnaryOperator) -> &'static str {
            match op {
                UnaryOperator::Plus => "pos",
                UnaryOperator::Negate => "neg",
                UnaryOperator::Abs => unreachable!("ABS is a call"),
            }
        }
        fn comparison(op: Comparison) -> &'static str {
            match op {
                Comparison::Equal => "=",
                Comparison::NotEqual => "<>",
                Comparison::Less => "<",
                Comparison::LessEqual => "<=",
                Comparison::Greater => ">",
                Comparison::GreaterEqual => ">=",
            }
        }
        /// The README's fully parenthesized prefix form of one admitted parse.
        fn shape(compiled: &CompiledExpression, id: usize) -> String {
            let syntax = compiled.syntax();
            let text = syntax.text(id);
            let child = |id| shape(compiled, id);
            match &syntax.nodes()[id].kind {
                NodeKind::Number { fractional, text } => {
                    format!("({} {text})", if *fractional { "float" } else { "int" })
                }
                NodeKind::String(value) => format!("(str {})", quote(value)),
                NodeKind::Temporal { kind, value } => format!(
                    "({} {})",
                    match kind {
                        TemporalKind::Date => "date",
                        TemporalKind::DateTime => "datetime",
                    },
                    quote(value)
                ),
                NodeKind::Boolean(value) => value.to_string(),
                NodeKind::Null => "null".into(),
                NodeKind::Identifier => format!("(id {text})"),
                NodeKind::Star => format!("(star {})", text.strip_suffix(".*").unwrap()),
                NodeKind::Group(inner) => child(*inner),
                NodeKind::Not(inner) => format!("(not {})", child(*inner)),
                NodeKind::And(left, right) => format!("(and {} {})", child(*left), child(*right)),
                NodeKind::Or(left, right) => format!("(or {} {})", child(*left), child(*right)),
                NodeKind::Compare {
                    operator,
                    left,
                    right,
                } => format!(
                    "({} {} {})",
                    comparison(*operator),
                    child(*left),
                    child(*right)
                ),
                NodeKind::IsNull { value, negated } => format!(
                    "({} {})",
                    if *negated { "is-not-null" } else { "is-null" },
                    child(*value)
                ),
                NodeKind::In {
                    value,
                    items,
                    negated,
                } => format!(
                    "({} {} {})",
                    negated_name(*negated, "in"),
                    child(*value),
                    items
                        .iter()
                        .map(|&id| child(id))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                NodeKind::Between {
                    value,
                    lower,
                    upper,
                    negated,
                } => format!(
                    "({} {} {} {})",
                    negated_name(*negated, "between"),
                    child(*value),
                    child(*lower),
                    child(*upper)
                ),
                NodeKind::Like {
                    value,
                    pattern,
                    escape,
                    negated,
                } => format!(
                    "({} {} {}{})",
                    negated_name(*negated, "like"),
                    child(*value),
                    child(*pattern),
                    escape
                        .map(|c| format!(" (escape {})", quote(&c.to_string())))
                        .unwrap_or_default()
                ),
                NodeKind::Unary { operator, operand } => {
                    format!("({} {})", unary(*operator), child(*operand))
                }
                NodeKind::Binary {
                    operator,
                    left,
                    right,
                } => format!("({} {} {})", binary(*operator), child(*left), child(*right)),
                NodeKind::Call { name, arguments } => format!(
                    "(call {} {})",
                    syntax.slice(*name).to_ascii_uppercase(),
                    arguments
                        .iter()
                        .map(|&id| child(id))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                NodeKind::Reduction {
                    reducer, operand, ..
                } => format!("(reduce {} {})", reducer.name(), child(*operand)),
            }
        }
        fn negated_name(negated: bool, name: &str) -> String {
            if negated {
                format!("not-{name}")
            } else {
                name.into()
            }
        }
        fn template(parsed: &StringTemplate) -> String {
            let mut parts = Vec::new();
            let mut literal = String::new();
            for part in parsed.parts() {
                if let Part::Placeholder(index) = part {
                    if !literal.is_empty() {
                        parts.push(format!("(text {})", quote(&literal)));
                        literal.clear();
                    }
                    parts.push(format!("(placeholder {})", parsed.identifiers()[*index]));
                } else {
                    literal.push_str(parsed.literal(part).unwrap());
                }
            }
            if !literal.is_empty() {
                parts.push(format!("(text {})", quote(&literal)));
            }
            if parts.is_empty() {
                "(template)".into()
            } else {
                format!("(template {})", parts.join(" "))
            }
        }

        fn yaml(path: &str) -> Json {
            fn value(document: &Document, id: usize) -> Json {
                match &document.nodes()[id] {
                    DocumentNode::Null => Json::Null,
                    DocumentNode::Boolean(value) => json!(value),
                    DocumentNode::Integer(value) => {
                        yamaa_adapters::json_wire::decode(value).unwrap()
                    }
                    DocumentNode::Float(value) => json!(value),
                    DocumentNode::Text(value) => json!(value),
                    DocumentNode::Sequence(items) => {
                        Json::Array(items.iter().map(|&id| value(document, id)).collect())
                    }
                    DocumentNode::Mapping(items) => Json::Object(
                        items
                            .iter()
                            .map(|&(key, child)| {
                                let DocumentNode::Text(key) = &document.nodes()[key] else {
                                    panic!("fixture mapping keys must be text")
                                };
                                (key.clone(), value(document, child))
                            })
                            .collect(),
                    ),
                }
            }
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join(path);
            let bytes = std::fs::read(path).unwrap();
            let decoded = decode_yaml(&bytes).unwrap();
            value(&decoded.document, decoded.document.root())
        }

        fn context(name: &str) -> Context {
            match name {
                "predicate" => Context::Predicate,
                "aggregate" => Context::Aggregate,
                "derivation" => Context::Derivation,
                _ => panic!("unknown expression context: {name}"),
            }
        }

        fn observation(
            contract: &str,
            field: Option<&str>,
            text: &str,
        ) -> Result<(String, Vec<String>), String> {
            let (shape, mut identifiers) = match contract {
                "expression" => match compile(
                    text,
                    context(field.expect("an expression case states its context")),
                    "columns.RESULT.derivation.expr",
                ) {
                    Ok(compiled) => (
                        shape(&compiled, compiled.syntax().root()),
                        compiled.identifiers().map(String::from).collect::<Vec<_>>(),
                    ),
                    Err(CompileError::Invalid { failure, .. }) => {
                        return Err(failure.condition().into())
                    }
                    Err(error) => panic!("grammar vector exceeds resource policy: {error:?}"),
                },
                "string-template" => {
                    let parsed = StringTemplate::new(
                        text.into(),
                        "columns.RESULT.str_template".into(),
                        None,
                    )
                    .unwrap();
                    if let Some(diagnostic) = parsed.declaration_diagnostic() {
                        return Err(diagnostic.definition().condition.into());
                    }
                    (template(&parsed), parsed.identifiers().to_vec())
                }
                _ => panic!("unknown grammar: {contract}"),
            };
            identifiers.sort();
            Ok((shape, identifiers))
        }

        fn grammar_case(contract: &str, case: &Json) {
            let text = case["text"].as_str().unwrap();
            let label = format!(
                "{contract}/{}",
                yamaa_adapters::json_wire::encode(&case["id"])
            );
            let actual = observation(contract, case["context"].as_str(), text);
            if case["parse"] == "reject" {
                assert_eq!(
                    actual.unwrap_err(),
                    case["condition"].as_str().unwrap(),
                    "{label}: {text}"
                );
                return;
            }
            assert_eq!(case["parse"], "accept", "{label}");
            let (shape, identifiers) =
                actual.unwrap_or_else(|condition| panic!("{label}: {text}: {condition}"));
            if let Some(expected) = case["shape"].as_str() {
                assert_eq!(
                    shape,
                    expected.split_whitespace().collect::<Vec<_>>().join(" "),
                    "{label}"
                );
            }
            if let Some(expected) = case["identifiers"].as_array() {
                let expected = expected
                    .iter()
                    .map(|name| name.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>();
                assert_eq!(identifiers, expected, "{label}");
            }
        }

        /// Expand the closed vocabularies into independent acceptance boundaries.
        /// The same expansion is staged for the installed host suites.
        fn expansions(document: &Json) -> Vec<Json> {
            let mut cases = Vec::new();
            let mut case = |context: &str, text: String, accepted: bool, condition: &str| {
                cases.push(json!({"id":format!("{context}: {text}"), "context":context, "text":text, "parse":if accepted { "accept" } else { "reject" }, "condition":condition}));
            };
            let contexts = ["predicate", "aggregate", "derivation"];
            for (name, arity) in document["vocabulary"]["function"].as_object().unwrap() {
                let lower = arity["min_arguments"].as_u64().unwrap() as usize;
                let upper = arity["max_arguments"].as_u64().map(|value| value as usize);
                let numeric = arity["result"] == "numeric";
                let mut counts = BTreeSet::from([0, lower - 1, lower, lower + 1, 8]);
                if let Some(upper) = upper {
                    counts.insert(upper + 1);
                }
                for count in counts {
                    let fits = count >= lower && upper.is_none_or(|upper| count <= upper);
                    let call = |pattern: bool| {
                        let arguments = (0..count)
                            .map(|index| if pattern && index > 0 { "'a'" } else { "A" })
                            .collect::<Vec<_>>();
                        format!("{}({})", name.to_ascii_lowercase(), arguments.join(", "))
                    };
                    for context in contexts {
                        match (context, numeric) {
                            ("predicate", true) => case(
                                context,
                                format!("{} = 1", call(false)),
                                fits,
                                "prohibited_function",
                            ),
                            ("predicate", false) => {
                                case(context, call(true), fits, "prohibited_function")
                            }
                            (_, true) => case(context, call(false), fits, "prohibited_function"),
                            (_, false) => case(context, call(false), false, "prohibited_function"),
                        }
                    }
                }
            }
            for word in document["reserved"].as_array().unwrap() {
                let word = word.as_str().unwrap();
                let literal = word == "NULL";
                let truth = matches!(word, "TRUE" | "FALSE");
                case(
                    "predicate",
                    format!("{} = 1", word.to_ascii_lowercase()),
                    literal,
                    "invalid_expression",
                );
                case("predicate", format!("D.{word} = 1"), true, "");
                for context in ["aggregate", "derivation"] {
                    case(
                        context,
                        format!("1 + {}", word.to_ascii_lowercase()),
                        literal,
                        if truth {
                            "prohibited_construct"
                        } else {
                            "invalid_expression"
                        },
                    );
                    case(context, format!("D.{word} + 1"), true, "");
                }
            }
            let compare = document["productions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["name"] == "compare")
                .unwrap();
            for operator in compare["definition"]
                .as_str()
                .unwrap()
                .split('"')
                .skip(1)
                .step_by(2)
            {
                case("predicate", format!("A {operator} 1"), true, "");
                for context in ["aggregate", "derivation"] {
                    case(
                        context,
                        format!("A {operator} 1"),
                        false,
                        "prohibited_construct",
                    );
                }
            }
            for (name, declaration) in document["vocabulary"]["reducer"].as_object().unwrap() {
                let star = declaration["argument"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|value| value == "star");
                for argument in ["A", "A+1", "ABS(A)", "D.*", "", "A,B", "SUM(A)"] {
                    let text = format!("{}({argument})", name.to_ascii_lowercase());
                    let syntax = matches!(argument, "" | "A,B") || (argument == "D.*" && !star);
                    let condition = |admitted: &'static str| {
                        if syntax {
                            "invalid_expression"
                        } else {
                            admitted
                        }
                    };
                    case(
                        "aggregate",
                        text.clone(),
                        !syntax && argument != "SUM(A)",
                        condition("nested_reduction"),
                    );
                    case(
                        "derivation",
                        text.clone(),
                        false,
                        condition("prohibited_function"),
                    );
                    case(
                        "predicate",
                        format!("{text} = 1"),
                        false,
                        condition("prohibited_function"),
                    );
                }
            }
            cases
        }

        #[test]
        fn shared_grammars_have_exact_shapes_dependencies_decisions_and_vocabularies() {
            // Seam: a loop over authored truth, each case of the shared grammar
            // documents (`yaml/grammar/`) and the expansion of their closed
            // vocabularies, observed through the compiled parse's shape.
            let documents = [("expression", "formulas"), ("string-template", "text")].map(
                |(contract, rule)| {
                    let document = yaml(&format!("yaml/grammar/{contract}.yaml"));
                    assert_eq!(document["contract"], contract);
                    assert_eq!(document["rule"], rule);
                    (contract, document)
                },
            );
            for (contract, document) in &documents {
                for case in document["cases"].as_array().unwrap() {
                    grammar_case(contract, case);
                }
            }
            let expression = &documents[0].1;
            for case in expansions(expression) {
                grammar_case("expression", &case);
            }
            let reserved = expression["reserved"]
                .as_array()
                .unwrap()
                .iter()
                .map(|name| name.as_str().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(reserved, RESERVED);
        }

        fn regex_observation(pattern: &str, subject: &str) -> Json {
            use yamaa_core::portable_regex::{CompileError, Pattern};
            match Pattern::compile(pattern) {
                Err(CompileError::Invalid { .. }) => json!({"accepted":false}),
                Err(error) => panic!("regex truth exceeds compile policy: {error:?}"),
                Ok(pattern) => {
                    let found = pattern.search(subject);
                    assert_eq!(found.is_some(), pattern.is_match(subject), "{subject:?}");
                    json!({"accepted":true, "group_count":pattern.group_count(), "full_match":pattern.is_full_match(subject), "match":found.map(|found|found.groups)})
                }
            }
        }

        /// The replacement consumer itself, not a second scan written for the test.
        fn replace_observation(pattern: &str, subject: &str, replacement: &str) -> String {
            use yamaa_core::{
                table::ValueRef,
                text_replace::{Step, StringReplace},
            };
            let operation = StringReplace::new(
                "SRC.TEXT".into(),
                "columns.TEXT.derivation.str_replace".into(),
                pattern.into(),
                replacement.into(),
                None,
            )
            .unwrap();
            match operation.apply(ValueRef::Str(subject)) {
                Ok(Step::Matched { subject, spans }) => operation.render(subject, &spans).unwrap(),
                other => panic!("replacement truth exceeds policy: {other:?}"),
            }
        }

        #[test]
        fn unchanged_regex_documents_and_authored_capture_controls_run_directly() {
            // Seam: a loop over authored truth, `yaml/conformance/regex.yaml` and
            // `fixtures/regex_contract.tsv`, observed through the regex kernel and
            // the replacement operation themselves.
            let document = yaml("yaml/conformance/regex.yaml");
            assert_eq!(document["contract"], "regex");
            assert_eq!(document["contract_version"], "3.0.0");
            assert!(document.get("engine").is_none());
            let mut ids = BTreeSet::new();
            let mut covered = BTreeSet::new();
            let mut replaced = 0;
            for case in document["cases"].as_array().unwrap() {
                let id = case["id"].as_str().unwrap();
                assert!(
                    !id.is_empty() && ids.insert(id),
                    "duplicate/empty regex case {id}"
                );
                let covers = case["covers"].as_array().unwrap();
                assert!(!covers.is_empty(), "{id}");
                for tag in covers {
                    let tag = tag.as_str().unwrap();
                    assert!(
                        matches!(
                            tag,
                            "anchors"
                                | "capture-groups"
                                | "dialect"
                                | "empty-match"
                                | "escaping"
                                | "search"
                                | "unicode"
                                | "unsupported"
                        ),
                        "{id}: unknown coverage tag {tag}"
                    );
                    covered.insert(tag);
                }
                let actual = regex_observation(
                    case["pattern"].as_str().unwrap(),
                    case["subject"].as_str().unwrap_or(""),
                );
                if case["invalid"] == true {
                    assert_eq!(
                        actual,
                        json!({"accepted":false}),
                        "{}",
                        yamaa_adapters::json_wire::encode(&case["id"])
                    );
                    continue;
                }
                assert_eq!(
                    actual["accepted"],
                    true,
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["id"])
                );
                assert_eq!(
                    actual["full_match"],
                    case["schema_pattern"],
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["id"])
                );
                assert_eq!(
                    !actual["match"].is_null(),
                    case["matches"].as_bool().unwrap(),
                    "{}",
                    yamaa_adapters::json_wire::encode(&case["id"])
                );
                let extraction = &case["str_extract"];
                if extraction.is_null() {
                    assert!(
                        actual["match"].is_null(),
                        "{}",
                        yamaa_adapters::json_wire::encode(&case["id"])
                    );
                } else {
                    assert_eq!(
                        actual["match"][extraction["group"].as_u64().unwrap() as usize],
                        extraction["value"],
                        "{}",
                        yamaa_adapters::json_wire::encode(&case["id"])
                    );
                }
                let replacement = &case["str_replace"];
                if !replacement.is_null() {
                    assert_eq!(
                        replace_observation(
                            case["pattern"].as_str().unwrap(),
                            case["subject"].as_str().unwrap(),
                            replacement["replacement"].as_str().unwrap(),
                        ),
                        replacement["value"].as_str().unwrap(),
                        "{}",
                        yamaa_adapters::json_wire::encode(&case["id"])
                    );
                    replaced += 1;
                }
            }
            assert!(replaced > 0, "no regex case records a replacement");
            // Retain the original fixture-integrity coverage requirement alongside
            // actual parser/matcher assertions, without a Python replay executable.
            for required in [
                "anchors",
                "capture-groups",
                "empty-match",
                "escaping",
                "unicode",
                "unsupported",
            ] {
                assert!(
                    covered.contains(required),
                    "no regex case covers {required}"
                );
            }
            for line in include_str!("fixtures/regex_contract.tsv").lines().skip(1) {
                let fields = line.split('\t').collect::<Vec<_>>();
                assert_eq!(fields.len(), 4);
                let pattern: String = serde_json::from_str(fields[1]).unwrap();
                let subject: String = serde_json::from_str(fields[2]).unwrap();
                let expected: Json = yamaa_adapters::json_wire::decode(fields[3]).unwrap();
                assert_eq!(
                    regex_observation(&pattern, &subject),
                    expected,
                    "{}",
                    fields[0]
                );
            }
        }

        #[test]
        fn regex_scalar_membership_and_greedy_lazy_counts_keep_independent_set_truth() {
            // Seam: the regex kernel over every code point of each class and
            // every counted repetition, generated here from independent sets.
            let whitespace = BTreeSet::from([
                0x9, 0xa, 0xb, 0xc, 0xd, 0x20, 0xa0, 0x1680, 0x2000, 0x2001, 0x2002, 0x2003,
                0x2004, 0x2005, 0x2006, 0x2007, 0x2008, 0x2009, 0x200a, 0x2028, 0x2029, 0x202f,
                0x205f, 0x3000, 0xfeff,
            ]);
            let word = (48..=57)
                .chain(65..=90)
                .chain(97..=122)
                .chain([95])
                .collect::<BTreeSet<_>>();
            let digits = (48..=57).collect::<BTreeSet<_>>();
            let line_terminators = BTreeSet::from([10, 13, 0x2028, 0x2029]);
            let empty = BTreeSet::new();
            let points = (0..128)
                .chain(whitespace.iter().copied())
                .chain([0x85, 0xe9, 0x301, 0x665, 0x1d400, 0x10ffff])
                .collect::<BTreeSet<_>>();
            for (spelling, members, negative) in [
                (r"\s", &whitespace, false),
                (r"[\s]", &whitespace, false),
                (r"\S", &whitespace, true),
                (r"[\S]", &whitespace, true),
                (r"[^\S]", &whitespace, false),
                (r"\w", &word, false),
                (r"\W", &word, true),
                (r"\d", &digits, false),
                (r"\D", &digits, true),
                (".", &line_terminators, true),
                ("[]", &empty, false),
                ("[^]", &empty, true),
            ] {
                for point in &points {
                    let subject = char::from_u32(*point).unwrap().to_string();
                    let matches = members.contains(point) != negative;
                    let expected = json!({"accepted":true,"group_count":0,"full_match":matches,"match":if matches { Some(vec![subject.as_str()]) } else { None }});
                    assert_eq!(
                        regex_observation(&format!("^{spelling}$"), &subject),
                        expected,
                        "{spelling}: {point:x}"
                    );
                }
            }
            for minimum in 0..5 {
                for maximum in minimum..5 {
                    for length in 0..7 {
                        for lazy in [false, true] {
                            let subject = "a".repeat(length);
                            let suffix = if lazy { "?" } else { "" };
                            let pattern = format!("(a{{{minimum},{maximum}}}{suffix})");
                            let captured =
                                "a".repeat(if lazy { minimum } else { maximum.min(length) });
                            let expected = json!({"accepted":true,"group_count":1,"full_match":minimum<=length&&length<=maximum,"match":if length<minimum { None } else { Some(vec![captured.as_str(),captured.as_str()]) }});
                            assert_eq!(
                                regex_observation(&pattern, &subject),
                                expected,
                                "{pattern}: {subject}"
                            );
                        }
                    }
                }
            }
        }

        #[test]
        fn unchanged_independent_rational_rounding_corpus_matches_the_rounding_operation() {
            // Seam: a loop over authored truth, `fixtures/rounding_rational.tsv`,
            // observed through the rounding operation's float bits.
            use yamaa_core::{dataset::Rounding, table::ValueRef, value::Value};
            // Original cb34f38a3:rust/tools/assess_rounding.py, using
            // Fraction(abs(value)) and the exact FORMULA-16 half-minus-2^-26 threshold.
            // All 2,936 unchanged boundary/range/seed-1585 inputs and expected bits
            // were copied from that independent rational oracle, never a native run.
            let mut seen = BTreeSet::new();
            for line in include_str!("fixtures/rounding_rational.tsv")
                .lines()
                .skip(1)
            {
                let fields = line.split('\t').collect::<Vec<_>>();
                assert_eq!(fields.len(), 4);
                assert!(seen.insert(fields[0]));
                let value = f64::from_bits(u64::from_str_radix(fields[1], 16).unwrap());
                let rounding = Rounding::new("A", "rounding", fields[2]).unwrap();
                let actual = match rounding
                    .apply(ValueRef::from(&Value::float(value)))
                    .unwrap()
                {
                    Value::Missing => "missing".into(),
                    Value::Float(value) => format!("{:016x}", value.get().to_bits()),
                    other => panic!("rounding must retain float output: {other:?}"),
                };
                assert_eq!(actual, fields[3], "{}", fields[0]);
            }
            assert_eq!(seen.len(), 2936);
        }

        use crate::support::{
            aggregate_syntax as aggregate_fixture, numeric_syntax as numeric_fixture,
            predicate_syntax as predicate_fixture,
        };

        fn syntax_outcome(family: &str, text: &str) -> Json {
            let mut observed = match family {
                "numeric" => numeric_fixture::observe(text),
                "aggregate" => aggregate_fixture::observe(text),
                "predicate" => predicate_fixture::observe(text),
                _ => panic!("unknown original syntax family"),
            };
            observed["outcome"].take()
        }

        #[test]
        fn original_authored_syntax_controls_retain_complete_ast_metadata_and_diagnostics() {
            // Seam: a loop over authored truth, `fixtures/syntax_controls.json`,
            // observed through the syntax projections of `support::*_syntax`.
            // Literal controls from cb34f38a3:rust/tools/native_syntax_checks.py.
            let cases: Vec<Json> = yamaa_adapters::json_wire::decode_array(include_str!(
                "fixtures/syntax_controls.json"
            ))
            .unwrap();
            assert_eq!(cases.len(), 25);
            for case in cases {
                let family = case["family"].as_str().unwrap();
                let text = case["text"].as_str().unwrap();
                assert_eq!(
                    syntax_outcome(family, text),
                    case["expected"],
                    "{family}: {text}"
                );
            }
        }

        #[test]
        fn original_robustness_inputs_keep_complete_native_outcomes_and_unicode_positions() {
            // Seam: a loop over authored truth, `fixtures/syntax_robustness.tsv`,
            // whose outcomes this test checks for their shape and Unicode
            // positions.
            let mut counts = std::collections::BTreeMap::new();
            for line in include_str!("fixtures/syntax_robustness.tsv")
                .lines()
                .skip(1)
            {
                let (family, encoded) = line.split_once('\t').unwrap();
                let text: String = serde_json::from_str(encoded).unwrap();
                *counts.entry(family).or_insert(0) += 1;
                let actual = syntax_outcome(family, &text);
                match actual["status"].as_str().unwrap() {
                    "parsed" => {
                        assert!(actual["ast"].is_object());
                        assert!(actual["identifiers"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .all(Json::is_string));
                        if family == "aggregate" {
                            for field in ["star_datasets", "ungrouped_identifiers"] {
                                assert!(actual[field]
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .all(Json::is_string));
                            }
                        }
                    }
                    "invalid" | "resource_limit" | "unsupported" => {
                        let character = actual["position"]["character"].as_u64().unwrap() as usize;
                        let byte = actual["position"]["byte"].as_u64().unwrap() as usize;
                        assert!(character <= text.chars().count(), "{family}: {text}");
                        assert_eq!(
                            byte,
                            text.chars()
                                .take(character)
                                .map(char::len_utf8)
                                .sum::<usize>(),
                            "{family}: {text}"
                        );
                        if actual["status"] == "invalid" {
                            assert!(actual["context"].is_object());
                            let rule = actual["rule"].as_str().unwrap();
                            let (prefix, number) = rule.split_once('-').unwrap();
                            assert!(
                                !prefix.is_empty()
                                    && prefix.bytes().all(|byte| byte.is_ascii_uppercase())
                                    && number.parse::<u32>().is_ok(),
                                "{family}: {text}: {rule} is not a card ID"
                            );
                        }
                    }
                    _ => panic!(
                        "incomplete native syntax observation: {family}: {text}: {}",
                        yamaa_adapters::json_wire::encode(&actual)
                    ),
                }
            }
            assert_eq!(
                counts,
                std::collections::BTreeMap::from([
                    ("numeric", 2308),
                    ("aggregate", 3014),
                    ("predicate", 1016)
                ])
            );
        }

        #[test]
        fn original_numeric_and_aggregate_ast_spans_keep_the_authored_expression_sites() {
            // Seam: the spans of a parsed expression's syntax tree, which the
            // syntax projections return and no case states.
            for (expression, observe) in [
                (
                    "A + POWER(B, 2) / -3",
                    numeric_fixture::observe_with_spans as fn(&str) -> Json,
                ),
                (
                    "SUM(EX.DOSE) / NULLIF(EX.PLANDOSE, 0)",
                    aggregate_fixture::observe_with_spans as fn(&str) -> Json,
                ),
            ] {
                let actual = observe(expression);
                assert_eq!(actual["outcome"]["status"], "parsed");
                assert_eq!(
                    actual["outcome"]["ast"]["span"],
                    json!([0, expression.chars().count()])
                );
            }
            let invalid = numeric_fixture::observe_with_spans("SUM(AVAL)");
            assert_eq!(invalid["outcome"]["condition"], "prohibited_function");
            assert_eq!(invalid["outcome"]["span"], json!([0, 3]));
        }

        #[test]
        fn closed_keywords_and_refused_constructs_keep_their_names() {
            // Seam: the closed keywords and comparison operators of
            // `yaml/grammar/expression.yaml`, and the construct the aggregate
            // and derivation parsers each name in a refusal.
            let document = yaml("yaml/grammar/expression.yaml");
            assert!(document.get("prohibited").is_none());
            let reserved = document["reserved"]
                .as_array()
                .unwrap()
                .iter()
                .map(|name| name.as_str().unwrap())
                .collect::<BTreeSet<_>>();
            assert_eq!(
                reserved,
                BTreeSet::from([
                    "AND", "BETWEEN", "FALSE", "IN", "IS", "LIKE", "NOT", "NULL", "OR", "TRUE"
                ])
            );
            let comparison = document["productions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| value["name"] == "compare")
                .unwrap();
            assert_eq!(
                comparison["definition"]
                    .as_str()
                    .unwrap()
                    .split('"')
                    .skip(1)
                    .step_by(2)
                    .collect::<Vec<_>>(),
                ["=", "<>", "<", "<=", ">", ">="]
            );
            for (text, construct) in [
                ("A AND B", "boolean"),
                ("A OR B", "boolean"),
                ("NOT A", "boolean"),
                ("TRUE", "boolean"),
                ("A = B", "comparison"),
                ("A IN (1)", "comparison"),
                ("A NOT BETWEEN 1 AND 2", "comparison"),
                ("A LIKE B", "comparison"),
                ("A IS NOT NULL", "comparison"),
                ("'text'", "string"),
                ("DATE '2025-06-01'", "temporal"),
            ] {
                let actual = syntax_outcome("aggregate", text);
                assert_eq!(
                    actual["condition"], "prohibited_construct",
                    "aggregate: {text}"
                );
                assert_eq!(
                    actual["context"]["construct"], construct,
                    "aggregate: {text}"
                );
                // An expression field admits quoted text and temporal literals.
                let refused = match compile(text, Context::Derivation, "columns.RESULT.derivation")
                {
                    Ok(_) => None,
                    Err(CompileError::Invalid {
                        failure: Failure::ProhibitedConstruct { construct: found },
                        ..
                    }) => Some(found.name()),
                    Err(error) => panic!("derivation: {text}: {error:?}"),
                };
                assert_eq!(
                    refused,
                    Some(construct).filter(|name| !matches!(*name, "string" | "temporal")),
                    "derivation: {text}"
                );
            }
            for (name, declaration) in document["vocabulary"]["reducer"].as_object().unwrap() {
                assert_eq!(
                    declaration["argument"],
                    if name == "COUNT" {
                        json!(["expression", "star"])
                    } else {
                        json!(["expression"])
                    }
                );
            }
        }
    }
    mod odm_declaration_fixture {
        //! The ODM declaration projection's catalog. Its refusals are cases of
        //! `fixtures/diagnostics/declaration_vectors.json`.
        use crate::support::odm_declaration as fixture;
        use yamaa_adapters::json;
        use yamaa_core::json::Value;

        fn spec(derivation: Value) -> Value {
            json!({"yamaa":"1.0", "type":"spec", "domain":"RESULT", "keys":["USUBJID"],
        "input":{"ODM":"never-read.csv"},
        "output":{"path":"never-written.csv","columns":["USUBJID","AGE"]},
        "columns":[{"name":"USUBJID","type":"str","label":"Subject", "derivation":"ODM.SubjectKey"},
            {"name":"AGE","type":"int","label":"Age","derivation":derivation}]})
        }
        fn header() -> Vec<String> {
            [
                "StudyOID",
                "MetaDataVersionOID",
                "SubjectKey",
                "StudyEventOID",
                "StudyEventRepeatKey",
                "FormOID",
                "FormRepeatKey",
                "ItemGroupOID",
                "ItemGroupRepeatKey",
                "ItemOID",
                "Value",
            ]
            .map(String::from)
            .into()
        }
        fn observe(spec: Value, headers: &[Vec<String>]) -> Value {
            fixture::observe(&yamaa_adapters::json_wire::encode(&spec), headers)
        }

        #[test]
        fn canonical_catalog_excludes_vendor_fields_and_header_case_is_native_owned() {
            // Seam: the catalog and field list the ODM declaration projection
            // (`support::odm_declaration`) observes, which no public call
            // returns.
            let mut names = header()
                .iter()
                .map(|name| name.to_uppercase())
                .collect::<Vec<_>>();
            names.push("SITEID".into());
            let observed = observe(spec(json!({"odm":"ODM.IT.DM.AGE"})), &[names]);
            assert_eq!(observed["status"], "prepared");
            assert_eq!(observed["binding"]["status"], "bound");
            assert_eq!(observed["fields"], json!(header()));
            assert_eq!(
                observed["catalog"]["ODM"],
                json!(header()
                    .iter()
                    .map(|name| (name, "str"))
                    .collect::<Vec<_>>())
            );
        }
    }
    mod original_intermediate_verifications {
        //! Shipped-schema successors for specification/test_intermediate_verification.py
        //! retired in #2110. The admitted and absent verifications, the restated
        //! list and the singular field are cases of
        //! `fixtures/diagnostics/declaration_vectors.json`;
        //! the invalid payloads stay here, private Python model instances replaced
        //! by the native check boundary.
        use std::fs;
        use yamaa_adapters::json;
        use yamaa_application::file_application;
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;
        use yamaa_core::json::Value;

        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let path = crate::support::Scratch::new("original-intermediate-verification");
                Self(path)
            }
            fn check(&self, intermediate: Value) -> Vec<Diagnostic> {
                let document = json!({
                    "yamaa":"1.0", "type":"spec", "domain":"OUT", "base":"DS",
                    "keys":["STUDYID","USUBJID"],
                    "input":{"DS":{"path":"never-read.csv","types":{"STUDYID":"str","USUBJID":"str"}}},
                    "output":{"path":"out.csv","columns":["STUDYID","USUBJID"]},
                    "columns":[
                        {"name":"STUDYID","type":"str","derivation":"DS.STUDYID"},
                        {"name":"USUBJID","type":"str","derivation":"DS.USUBJID"}
                    ],
                    "intermediates":[intermediate]
                });
                let entry = self.0.join("spec.yaml");
                fs::write(
                    &entry,
                    yamaa_adapters::json_wire::encode(&document).into_bytes(),
                )
                .unwrap();
                let entry = entry.to_str().unwrap();
                let issues = file_application::check(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: entry,
                        environment: None,
                    },
                    crate::support::study::identity("original-intermediate-verification", entry),
                );
                assert!(!self.0.join("never-read.csv").exists());
                assert!(!self.0.join("out.csv").exists());
                issues
            }
        }

        #[test]
        fn all_original_invalid_intermediate_unique_payloads_are_refused() {
            // Seam: a property of every issue each refusal reports (the
            // validation phase, no missing resource, spec paths under
            // `intermediates`), which the case form's rows do not state.
            let study = Study::new();
            for verification in [
                json!({"unique":[]}),
                json!({"unique":{"columns":[]}}),
                json!({"unique":{"columns":["STUDYID"],"severity":"warning"}}),
                json!({"assert":{"require":"TRUE"}}),
            ] {
                let issues = study.check(json!({
                    "id":"DS_EOS", "dataset":"DS", "verifications":[verification]
                }));
                assert!(
                    !issues.is_empty(),
                    "{}",
                    yamaa_adapters::json_wire::encode(&verification)
                );
                assert!(
                    issues.iter().all(|issue| issue.phase() == "validation"
                        && issue.condition() != "resource_path_missing"
                        && issue
                            .spec_paths
                            .iter()
                            .all(|path| path.starts_with("intermediates"))),
                    "{}: {issues:?}",
                    yamaa_adapters::json_wire::encode(&verification)
                );
            }
        }
    }
}
