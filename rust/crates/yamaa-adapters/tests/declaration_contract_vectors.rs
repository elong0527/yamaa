#![cfg(feature = "yaml")]
//! Declaration contracts.
#[allow(dead_code)]
mod support;
mod declaration {
    mod contract_vectors {
        //! Direct native checks of unchanged authored grammar, regex and rounding truth.
        use a::Reducer;
        use n::NumericFunction;
        use std::collections::BTreeSet;
        use yamaa_adapters::yaml_decode::decode_yaml;
        use yamaa_core::json::{json, Value as Json};
        use yamaa_core::schema::{Document, DocumentNode};
        use yamaa_core::{
            aggregate_parser as a,
            numeric::{BinaryOperator, UnaryOperator},
            numeric_parser as n,
            predicate::Comparison,
            predicate_parser as p,
            text_template::{Part, StringTemplate},
        };

        // Exhaustive matches force a new native variant to enter the YAML inventory.
        macro_rules! vocabulary {
    ($name:ident, $kind:ident, [$($variant:ident),+]) => {
        fn $name() -> Vec<&'static str> {
            fn name(value: $kind) -> &'static str {
                match value { $($kind::$variant => value.name()),+ }
            }
            vec![$(name($kind::$variant)),+]
        }
    };
}
        vocabulary!(
            numeric_vocabulary,
            NumericFunction,
            [
                Abs,
                Ceil,
                Floor,
                Trunc,
                Sqrt,
                Power,
                Exp,
                Ln,
                Mod,
                Greatest,
                Least,
                NullIf,
                Coalesce,
                RoundHalfAwayFromZero
            ]
        );
        vocabulary!(
            aggregate_vocabulary,
            Reducer,
            [Sum, Count, Min, Max, Mean, Only]
        );

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
        fn numeric(parsed: &n::ParsedNumeric, id: usize) -> String {
            let node = &parsed.nodes()[id];
            let text = &parsed.expression()[node.span.start..node.span.end];
            match &node.kind {
                n::ParsedKind::Number { fractional } => {
                    format!("({} {text})", if *fractional { "float" } else { "int" })
                }
                n::ParsedKind::Null => "null".into(),
                n::ParsedKind::Identifier => format!("(id {text})"),
                n::ParsedKind::Group { operand } => numeric(parsed, *operand),
                n::ParsedKind::Unary { operator, operand } => {
                    format!("({} {})", unary(*operator), numeric(parsed, *operand))
                }
                n::ParsedKind::Binary {
                    operator,
                    left,
                    right,
                } => format!(
                    "({} {} {})",
                    binary(*operator),
                    numeric(parsed, *left),
                    numeric(parsed, *right)
                ),
                n::ParsedKind::Call {
                    function,
                    arguments,
                    ..
                } => format!(
                    "(call {} {})",
                    function.name(),
                    arguments
                        .iter()
                        .map(|&id| numeric(parsed, id))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
            }
        }
        fn aggregate(parsed: &a::ParsedAggregate, id: usize) -> String {
            let node = &parsed.nodes()[id];
            let text = &parsed.expression()[node.span.start..node.span.end];
            match &node.kind {
                a::ParsedKind::Number { fractional } => {
                    format!("({} {text})", if *fractional { "float" } else { "int" })
                }
                a::ParsedKind::Null => "null".into(),
                a::ParsedKind::Identifier => format!("(id {text})"),
                a::ParsedKind::Star => format!("(star {})", text.strip_suffix(".*").unwrap()),
                a::ParsedKind::Group { operand } => aggregate(parsed, *operand),
                a::ParsedKind::Unary { operator, operand } => {
                    format!("({} {})", unary(*operator), aggregate(parsed, *operand))
                }
                a::ParsedKind::Binary {
                    operator,
                    left,
                    right,
                } => format!(
                    "({} {} {})",
                    binary(*operator),
                    aggregate(parsed, *left),
                    aggregate(parsed, *right)
                ),
                a::ParsedKind::Call {
                    function,
                    arguments,
                    ..
                } => format!(
                    "(call {} {})",
                    function.name(),
                    arguments
                        .iter()
                        .map(|&id| aggregate(parsed, id))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                a::ParsedKind::Reduction {
                    reducer, operand, ..
                } => format!(
                    "(reduce {} {})",
                    reducer.name(),
                    aggregate(parsed, *operand)
                ),
            }
        }
        fn predicate(parsed: &p::ParsedPredicate, id: usize) -> String {
            let node = &parsed.nodes()[id];
            let text = &parsed.expression()[node.span.start..node.span.end];
            let child = |id| predicate(parsed, id);
            match &node.kind {
                p::ParsedKind::Number { fractional } => {
                    format!("({} {text})", if *fractional { "float" } else { "int" })
                }
                p::ParsedKind::String(value) => format!("(str {})", quote(value)),
                p::ParsedKind::Temporal { kind, value } => format!(
                    "({} {})",
                    match kind {
                        p::TemporalKind::Date => "date",
                        p::TemporalKind::DateTime => "datetime",
                    },
                    quote(value)
                ),
                p::ParsedKind::Null => "null".into(),
                p::ParsedKind::Identifier => format!("(id {text})"),
                p::ParsedKind::Boolean(value) => value.to_string(),
                p::ParsedKind::Group(id) => child(*id),
                p::ParsedKind::Not(id) => format!("(not {})", child(*id)),
                p::ParsedKind::And(left, right) => {
                    format!("(and {} {})", child(*left), child(*right))
                }
                p::ParsedKind::Or(left, right) => {
                    format!("(or {} {})", child(*left), child(*right))
                }
                p::ParsedKind::Compare {
                    operator,
                    left,
                    right,
                } => format!(
                    "({} {} {})",
                    match operator {
                        Comparison::Equal => "=",
                        Comparison::NotEqual => "<>",
                        Comparison::Less => "<",
                        Comparison::LessEqual => "<=",
                        Comparison::Greater => ">",
                        Comparison::GreaterEqual => ">=",
                    },
                    child(*left),
                    child(*right)
                ),
                p::ParsedKind::IsNull { value, negated } => format!(
                    "({} {})",
                    if *negated { "is-not-null" } else { "is-null" },
                    child(*value)
                ),
                p::ParsedKind::In {
                    value,
                    items,
                    negated,
                } => format!(
                    "({} {} {})",
                    if *negated { "not-in" } else { "in" },
                    child(*value),
                    items
                        .iter()
                        .map(|&id| child(id))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                p::ParsedKind::Between {
                    value,
                    lower,
                    upper,
                    negated,
                } => format!(
                    "({} {} {} {})",
                    if *negated { "not-between" } else { "between" },
                    child(*value),
                    child(*lower),
                    child(*upper)
                ),
                p::ParsedKind::Like {
                    value,
                    pattern,
                    escape,
                    negated,
                } => format!(
                    "({} {} {}{})",
                    if *negated { "not-like" } else { "like" },
                    child(*value),
                    child(*pattern),
                    escape
                        .map(|c| format!(" (escape {})", quote(&c.to_string())))
                        .unwrap_or_default()
                ),
                p::ParsedKind::Contains { source, pattern } => {
                    format!("(call STR_CONTAINS {} {})", child(*source), child(*pattern))
                }
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
                    DocumentNode::Integer(value) => serde_json::from_str(value).unwrap(),
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

        fn observation(contract: &str, text: &str) -> Result<(String, Vec<String>), String> {
            let (shape, mut identifiers) = match contract {
                "numeric" => match n::parse_numeric(text) {
                    Ok(parsed) => (
                        numeric(&parsed, parsed.root()),
                        parsed.identifiers().map(String::from).collect::<Vec<_>>(),
                    ),
                    Err(n::ParseError::Grammar { failure, .. }) => {
                        return Err(failure.condition().into())
                    }
                    Err(error) => panic!("grammar vector exceeds resource policy: {error:?}"),
                },
                "aggregate" => match a::parse_aggregate(text) {
                    Ok(parsed) => (
                        aggregate(&parsed, parsed.root()),
                        parsed.identifiers().map(String::from).collect::<Vec<_>>(),
                    ),
                    Err(a::ParseError::Grammar { failure, .. }) => {
                        return Err(failure.condition().into())
                    }
                    Err(error) => panic!("grammar vector exceeds resource policy: {error:?}"),
                },
                "predicate" => match p::parse_predicate(text) {
                    Ok(parsed) => (
                        predicate(&parsed, parsed.root()),
                        parsed.identifiers().map(String::from).collect::<Vec<_>>(),
                    ),
                    Err(p::ParseError::Grammar { failure, .. }) => {
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
            let label = format!("{contract}/{}", case["id"]);
            let actual = observation(contract, text);
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

        #[test]
        fn unchanged_shared_grammars_have_exact_shapes_dependencies_decisions_and_vocabularies() {
            let documents = [
                ("numeric", "operations/computation"),
                ("aggregate", "operations/aggregation"),
                ("predicate", "operations/predicates"),
                ("string-template", "operations/text"),
            ]
            .map(|(contract, rule)| {
                let document = yaml(&format!("yaml/grammar/{contract}.yaml"));
                assert_eq!(document["contract"], contract);
                assert_eq!(document["rule"], rule);
                (contract, document)
            });
            let numeric_doc = &documents[0].1;
            let vocabulary = |key: &str| {
                documents
                    .iter()
                    .find_map(|(contract, doc)| (*contract == key).then_some(doc))
                    .unwrap()
            };
            for (contract, document) in &documents {
                for case in document["cases"].as_array().unwrap() {
                    grammar_case(contract, case);
                }
                if matches!(*contract, "numeric" | "aggregate") {
                    for (name, arity) in numeric_doc["vocabulary"]["function"].as_object().unwrap()
                    {
                        let lower = arity["min_arguments"].as_u64().unwrap() as usize;
                        let upper = arity["max_arguments"].as_u64().map(|value| value as usize);
                        let mut counts = BTreeSet::from([0, lower - 1, lower, lower + 1, 8]);
                        if let Some(upper) = upper {
                            counts.insert(upper + 1);
                        }
                        for count in counts {
                            let accepted =
                                count >= lower && upper.is_none_or(|upper| count <= upper);
                            grammar_case(
                                contract,
                                &json!({
                                    "id":format!("arity-{name}-{count}"),
                                    "text":format!("{}({})", name.to_ascii_lowercase(), vec!["A"; count].join(", ")),
                                    "parse": if accepted { "accept" } else { "reject" },
                                    "condition":"prohibited_function"
                                }),
                            );
                        }
                    }
                    for word in numeric_doc["prohibited"].as_object().unwrap().keys() {
                        grammar_case(
                            contract,
                            &json!({"id":format!("prohibited-{word}"), "text":format!("A {} B",word.to_ascii_lowercase()), "parse":"reject", "condition":"prohibited_construct"}),
                        );
                        grammar_case(
                            contract,
                            &json!({"id":format!("qualified-{word}"), "text":format!("A.{word}"), "parse":"accept"}),
                        );
                    }
                }
                if *contract == "aggregate" {
                    for (name, declaration) in
                        document["vocabulary"]["reducer"].as_object().unwrap()
                    {
                        for argument in ["A", "A+1", "ABS(A)", "D.*", "", "A,B", "SUM(A)"] {
                            let accepted = matches!(argument, "A" | "A+1" | "ABS(A)")
                                || (argument == "D.*"
                                    && declaration["argument"]
                                        .as_array()
                                        .unwrap()
                                        .iter()
                                        .any(|value| value == "star"));
                            grammar_case(
                                contract,
                                &json!({"id":format!("reducer-{name}-{argument}"), "text":format!("{}({argument})",name.to_ascii_lowercase()), "parse":if accepted { "accept" } else { "reject" }, "condition":if argument == "SUM(A)" { "nested_reduction" } else { "invalid_aggregate_expression" }}),
                            );
                        }
                    }
                }
                if *contract == "predicate" {
                    for word in document["reserved"].as_array().unwrap() {
                        let word = word.as_str().unwrap();
                        grammar_case(
                            contract,
                            &json!({"id":format!("reserved-{word}"), "text":format!("{} = 1",word.to_ascii_lowercase()), "parse":if word == "NULL" { "accept" } else { "reject" }, "condition":"invalid_predicate", "identifiers":[]}),
                        );
                        grammar_case(
                            contract,
                            &json!({"id":format!("qualified-{word}"), "text":format!("D.{word} = 1"), "parse":"accept"}),
                        );
                    }
                    let comparison = document["productions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|value| value["name"] == "compare")
                        .unwrap();
                    for operator in comparison["definition"]
                        .as_str()
                        .unwrap()
                        .split('"')
                        .skip(1)
                        .step_by(2)
                    {
                        grammar_case(
                            contract,
                            &json!({"id":format!("comparison-{operator}"), "text":format!("A {operator} 1"), "parse":"accept"}),
                        );
                    }
                }
            }
            for (contract, key, names) in [
                ("numeric", "function", numeric_vocabulary()),
                ("aggregate", "reducer", aggregate_vocabulary()),
            ] {
                let actual = names.into_iter().collect::<BTreeSet<_>>();
                let declared = vocabulary(contract)["vocabulary"][key]
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>();
                assert_eq!(actual, declared, "{contract} closed native vocabulary");
            }
        }

        fn regex_observation(pattern: &str, subject: &str) -> Json {
            use yamaa_core::regex::{CompileError, Pattern};
            match Pattern::compile(pattern) {
                Err(CompileError::Invalid { .. }) => json!({"accepted":false}),
                Err(error) => panic!("regex truth exceeds compile policy: {error:?}"),
                Ok(pattern) => {
                    let found = pattern.search(subject).unwrap();
                    let full = pattern.full_match(subject).unwrap();
                    json!({"accepted":true, "group_count":pattern.group_count(), "full_match":full.is_some(), "match":found.map(|found|found.groups)})
                }
            }
        }

        #[test]
        fn unchanged_regex_documents_and_authored_capture_controls_run_directly() {
            let document = yaml("yaml/conformance/regex.yaml");
            assert_eq!(document["contract"], "regex");
            assert_eq!(document["contract_version"], "2.0.0");
            assert!(document.get("engine").is_none());
            let mut ids = BTreeSet::new();
            let mut covered = BTreeSet::new();
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
                    assert_eq!(actual, json!({"accepted":false}), "{}", case["id"]);
                    continue;
                }
                assert_eq!(actual["accepted"], true, "{}", case["id"]);
                assert_eq!(
                    actual["full_match"], case["schema_pattern"],
                    "{}",
                    case["id"]
                );
                assert_eq!(
                    !actual["match"].is_null(),
                    case["matches"].as_bool().unwrap(),
                    "{}",
                    case["id"]
                );
                let extraction = &case["str_extract"];
                if extraction.is_null() {
                    assert!(actual["match"].is_null(), "{}", case["id"]);
                } else {
                    assert_eq!(
                        actual["match"][extraction["group"].as_u64().unwrap() as usize],
                        extraction["value"],
                        "{}",
                        case["id"]
                    );
                }
            }
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
                let expected: Json = serde_json::from_str(fields[3]).unwrap();
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
        fn unchanged_independent_rational_rounding_corpus_matches_current_compiler() {
            use std::convert::Infallible;
            use yamaa_core::{
                evaluation::NumericResolver,
                numeric::Number,
                numeric_compiler::compile_numeric,
                value::{Selection, Value},
            };
            struct Inputs(f64, i64);
            impl NumericResolver for Inputs {
                type Error = Infallible;
                fn resolve(&mut self, name: &str) -> Result<Selection, Self::Error> {
                    Ok(Selection::Present(match name {
                        "A" => Value::float(self.0),
                        "D" => Value::Int(self.1),
                        _ => panic!("unexpected rounding operand"),
                    }))
                }
            }
            let plan = compile_numeric("ROUND_HALF_AWAY_FROM_ZERO(A, D)", "rounding").unwrap();
            // Original cb34f38a3:rust/tools/assess_rounding.py, using
            // Fraction(abs(value)) and the exact REQ-0418 half-minus-2^-26 threshold.
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
                let digits = fields[2].parse().unwrap();
                let actual = match plan.evaluate(&mut Inputs(value, digits)).unwrap() {
                    Number::Missing => "missing".into(),
                    Number::Float(value) => format!("{:016x}", value.get().to_bits()),
                    Number::Int(_) => panic!("rounding must retain float output"),
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
            // Literal controls from cb34f38a3:rust/tools/native_syntax_checks.py.
            let cases: Vec<Json> =
                serde_json::from_str(include_str!("fixtures/syntax_controls.json")).unwrap();
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
                            assert!(actual["requirement"].as_str().unwrap().starts_with("REQ-"));
                        }
                    }
                    _ => panic!("incomplete native syntax observation: {family}: {text}: {actual}"),
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
        fn original_closed_keyword_metadata_keeps_native_construct_classes() {
            let predicate_doc = yaml("yaml/grammar/predicate.yaml");
            let reserved = predicate_doc["reserved"]
                .as_array()
                .unwrap()
                .iter()
                .map(|name| name.as_str().unwrap())
                .collect::<BTreeSet<_>>();
            assert_eq!(
                reserved,
                BTreeSet::from([
                    "AND", "BETWEEN", "DATE", "DATETIME", "ESCAPE", "FALSE", "IN", "IS", "LIKE",
                    "NOT", "NULL", "OR", "TRUE"
                ])
            );
            let comparison = predicate_doc["productions"]
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
            let numeric_doc = yaml("yaml/grammar/numeric.yaml");
            let prohibited = json!({"AND":"boolean", "BETWEEN":"comparison", "CASE":"conditional", "ELSE":"conditional", "END":"conditional", "FALSE":"boolean", "IN":"comparison", "IS":"comparison", "LIKE":"comparison", "NOT":"boolean", "OR":"boolean", "OVER":"window", "THEN":"conditional", "TRUE":"boolean", "WHEN":"conditional"});
            assert_eq!(numeric_doc["prohibited"], prohibited);
            for (word, construct) in prohibited.as_object().unwrap() {
                for family in ["numeric", "aggregate"] {
                    let actual =
                        syntax_outcome(family, &format!("A {} B", word.to_ascii_lowercase()));
                    assert_eq!(actual["condition"], "prohibited_construct");
                    assert_eq!(&actual["context"]["construct"], construct);
                }
            }
            let aggregate_doc = yaml("yaml/grammar/aggregate.yaml");
            for (name, declaration) in aggregate_doc["vocabulary"]["reducer"].as_object().unwrap() {
                assert_eq!(
                    declaration["argument"],
                    if name == "COUNT" {
                        json!(["expr", "star"])
                    } else {
                        json!(["expr"])
                    }
                );
            }
        }
    }
    mod inheritance_capture {
        use std::{collections::BTreeMap, rc::Rc};
        use yamaa_adapters::{
            shipped_schema, specification_diagnostics,
            specification_source::{
                CapturedSchema, InheritanceError, InheritancePort, Source, SourceFailure,
            },
        };
        use yamaa_core::json::{json, Value};
        use yamaa_core::{inheritance as graph, inheritance_preparation as lifecycle};
        const VALID:&str="schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n";

        fn case_parent(width: usize) -> Vec<u8> {
            use std::fmt::Write;
            let mut document = String::from(
        "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\ncolumns:\n  - {name: ID, type: str, derivation: SRC.ID}\n",
    );
            for index in 1..width {
                writeln!(document, "  - name: K{index:04}\n    type: str\n    derivation:\n      case:\n        - when: \"SRC.A = 'x' AND SRC.B = '{}'\"\n          then: {{literal: P}}\n        - when: \"SRC.A IS NULL\"\n          then: {{literal: Q}}\n        - otherwise: SRC.A", index % 3).unwrap();
            }
            document.push_str("output: {path: out.csv, columns: [ID");
            for index in 1..width {
                write!(document, ", K{index:04}").unwrap();
            }
            document.push_str("]}\n");
            document.into_bytes()
        }

        #[test]
        fn wide_case_parent_empty_child_and_two_layer_chain_reach_model_admission() {
            let schema = shipped_schema::capture().unwrap();
            for width in [40, 64, 1024] {
                let parent = case_parent(width);
                let child = b"schema_version: '1.0'\nparents: [parent.yaml]\n";
                let grandchild = b"schema_version: '1.0'\nparents: [child.yaml]\n";
                for (name, bytes) in [
                    ("parent", parent.clone()),
                    ("explicit", [parent.as_slice(), b"parents: []\n"].concat()),
                    ("child", child.to_vec()),
                    ("grandchild", grandchild.to_vec()),
                ] {
                    let mut host = new_host(&parent);
                    host.bytes.insert("child.yaml".into(), child.to_vec());
                    match schema.prepare_document(source(bytes), "entry.yaml".into(), &mut host) {
                        Ok(prepared) => {
                            let document = prepared.model().document();
                            let columns = document.field(document.root(), "columns").unwrap();
                            let yamaa_core::schema::DocumentNode::Sequence(columns) =
                                &document.nodes()[columns]
                            else {
                                panic!("normalized columns");
                            };
                            assert_eq!(columns.len(), width, "{width}/{name}");
                        }
                        Err(InheritanceError::Entry(error)) => panic!("{width}/{name}: {error:?}"),
                        Err(error) => panic!("{width}/{name}: {}", outcome(error)),
                    }
                }
            }
        }

        #[test]
        fn repeated_parent_occurrences_cannot_bypass_the_traversal_request_bound() {
            let parents = vec!["parent.yaml"; 8193].join(", ");
            let entry = format!("schema_version: '1.0'\nparents: [{parents}]\n").into_bytes();
            let mut host = new_host(VALID.as_bytes());
            let result = shipped_schema::prepare(source(entry), "entry.yaml".into(), &mut host);
            let Err(InheritanceError::Preparation { error, .. }) = result else {
                panic!("pathological parent occurrences must be refused");
            };
            assert!(matches!(
                *error,
                lifecycle::Error::Traversal(graph::Error::Resource(
                    yamaa_core::limits::LimitExceeded {
                        resource: "parent_occurrences",
                        limit: 8192,
                        actual: Some(8193),
                    }
                ))
            ));
            assert_eq!(
                host.calls
                    .iter()
                    .filter(|call| call.starts_with("capture:"))
                    .count(),
                1
            );
            assert_eq!(
                host.calls
                    .iter()
                    .filter(|call| call.starts_with("resolve:"))
                    .count(),
                8192
            );
        }
        struct Payload {
            identity: Rc<()>,
            allocation: Box<u32>,
        }
        struct Host {
            bytes: BTreeMap<String, Vec<u8>>,
            calls: Vec<String>,
            failure: Option<Payload>,
            fail_at: usize,
        }
        impl InheritancePort for Host {
            type Error = Payload;
            fn canonicalize(
                &mut self,
                declaring: &str,
                written: &str,
            ) -> Result<graph::Source, graph::SourceError<Payload>> {
                self.calls.push(format!("resolve:{declaring}:{written}"));
                if self.calls.len() == self.fail_at {
                    return Err(graph::SourceError::Raised(self.failure.take().unwrap()));
                }
                Ok(graph::Source {
                    identity: written.into(),
                    display_path: format!("display/{written}"),
                })
            }
            fn capture(
                &mut self,
                source: &graph::Source,
            ) -> Result<Vec<u8>, graph::SourceError<Payload>> {
                self.calls.push(format!("capture:{}", source.identity));
                if self.calls.len() == self.fail_at {
                    return Err(graph::SourceError::Raised(self.failure.take().unwrap()));
                }
                Ok(self
                    .bytes
                    .remove(&source.identity)
                    .expect("no duplicate capture"))
            }
            fn rebase(
                &mut self,
                layer: &graph::Source,
                entry: &graph::Source,
                written: &str,
            ) -> Result<String, Payload> {
                self.calls.push(format!(
                    "rebase:{}:{}:{written}",
                    layer.identity, entry.identity
                ));
                if self.calls.len() == self.fail_at {
                    return Err(self.failure.take().unwrap());
                }
                Ok(written.into())
            }
        }
        fn new_host(bytes: &[u8]) -> Host {
            Host {
                bytes: [("parent.yaml".into(), bytes.to_vec())]
                    .into_iter()
                    .collect(),
                calls: vec![],
                failure: None,
                fail_at: usize::MAX,
            }
        }
        fn source(bytes: Vec<u8>) -> Source {
            Source {
                identity: "entry.yaml".into(),
                bytes,
            }
        }
        fn outcome(error: InheritanceError<Payload>) -> Value {
            match specification_diagnostics::inheritance_failure(error) {
                Ok(s) => serde_json::from_str(&s).unwrap(),
                Err(_) => panic!("semantic report"),
            }
        }
        fn schema() -> std::sync::Arc<CapturedSchema> {
            CapturedSchema::admit(shipped_schema::capture().unwrap().sources().to_vec(), 0).unwrap()
        }
        #[test]
        fn unchanged_independent_complete_original_preparation_reports_and_effect_counts() {
            for row in include_str!("fixtures/inheritance_preparation.tsv")
                .lines()
                .skip(1)
            {
                let fields: Vec<_> = row.split('\t').collect();
                assert_eq!(fields.len(), 5);
                let mut host = new_host(fields[2].as_bytes());
                let error = match shipped_schema::prepare(
                    source(fields[1].as_bytes().to_vec()),
                    "written/entry".into(),
                    &mut host,
                ) {
                    Err(e) => e,
                    Ok(_) => panic!("invalid independent original"),
                };
                assert_eq!(
                    outcome(error),
                    serde_json::from_str::<Value>(fields[4]).unwrap(),
                    "{}",
                    fields[0]
                );
                assert_eq!(host.calls[0], "resolve:entry.yaml:parent.yaml");
                assert_eq!(host.calls[1], "capture:parent.yaml");
                assert_eq!(
                    host.calls
                        .iter()
                        .filter(|c| c.starts_with("rebase:"))
                        .count(),
                    fields[3].parse::<usize>().unwrap()
                );
            }
        }
        #[test]
        fn actual_entry_and_parent_snapshots_are_moved_once_with_raw_positions_and_defaults() {
            let root = format!("{VALID}parents: [parent.yaml, parent.yaml]\n").into_bytes();
            let root_pointer = root.as_ptr();
            let mut host = new_host(b"schema_version: '1.0'\n");
            let parent_pointer = host.bytes["parent.yaml"].as_ptr();
            let prepared =
                match shipped_schema::prepare(source(root), "written/entry".into(), &mut host) {
                    Ok(p) => p,
                    Err(_) => panic!("valid inheritance"),
                };
            assert_eq!(prepared.source().bytes.as_ptr(), root_pointer);
            assert_eq!(prepared.parents().len(), 1);
            let parent = &prepared.parents()[0];
            assert_eq!(parent.source().bytes.as_ptr(), parent_pointer);
            assert_eq!(parent.source().bytes, b"schema_version: '1.0'\n");
            assert_eq!(
                parent.raw().locations.len(),
                parent.raw().document.nodes().len()
            );
            assert_eq!(
                prepared.raw().locations.len(),
                prepared.raw().document.nodes().len()
            );
            assert_eq!(
                host.calls
                    .iter()
                    .filter(|s| s.starts_with("resolve:"))
                    .count(),
                2
            );
            assert_eq!(
                host.calls
                    .iter()
                    .filter(|s| s.starts_with("capture:"))
                    .count(),
                1
            );
            let d = prepared.model().document();
            let inputs = d.field(d.root(), "input").unwrap();
            let input = d.field(inputs, "SRC").unwrap();
            let policy = d.field(input, "empty_string").unwrap();
            assert_eq!(
                d.nodes()[policy],
                yamaa_core::schema::DocumentNode::Text("missing".into())
            );
            let inherited = prepared.inheritance().unwrap();
            assert_eq!(inherited.layers()[0].source.identity, "parent.yaml");
            assert_eq!(inherited.layers()[1].source.identity, "entry.yaml");
            assert_eq!(inherited.origins().len(), d.nodes().len());
        }
        #[test]
        fn opaque_canonicalize_capture_and_rebase_errors_preserve_their_actual_allocation() {
            for fail_at in [1, 2, 3] {
                let mut host = new_host(b"schema_version: '1.0'\n");
                let identity = Rc::new(());
                let allocation = Box::new(97);
                let pointer = (&*allocation) as *const u32;
                host.failure = Some(Payload {
                    identity: identity.clone(),
                    allocation,
                });
                host.fail_at = fail_at;
                let root = format!("{VALID}parents: parent.yaml\n");
                let Err(InheritanceError::Preparation {
                    error,
                    captured_parents,
                    ..
                }) = shipped_schema::prepare(
                    source(root.into_bytes()),
                    "entry.yaml".into(),
                    &mut host,
                )
                else {
                    panic!()
                };
                let original = match *error {
                    lifecycle::Error::Traversal(graph::Error::Host(SourceFailure::Host(p)))
                    | lifecycle::Error::Path(SourceFailure::Host(p)) => p,
                    _ => panic!(),
                };
                assert!(Rc::ptr_eq(&original.identity, &identity));
                assert_eq!((&*original.allocation) as *const u32, pointer);
                assert_eq!(host.calls.len(), fail_at);
                assert_eq!(captured_parents.len(), usize::from(fail_at == 3));
            }
        }
        #[test]
        fn raw_parent_overruns_refuse_before_codec_and_keep_existing_capture_category() {
            let root = format!("{VALID}parents: parent.yaml\n");
            let parent = b"schema_version: '1.0'\n";
            let mut host = new_host(&vec![b' '; yamaa_core::limits::Limits::SPEC_BYTES + 1]);
            let error = match shipped_schema::prepare(
                source(root.as_bytes().to_vec()),
                "entry.yaml".into(),
                &mut host,
            ) {
                Err(e) => e,
                Ok(_) => panic!(),
            };
            assert_eq!(
                outcome(error),
                json!({"protocol":"specification/prototype","outcome":{"status":"resource_limit","stage":"capture","phase":"yaml_decode","resource":"spec_bytes","limit":yamaa_core::limits::Limits::SPEC_BYTES,"actual":yamaa_core::limits::Limits::SPEC_BYTES + 1}})
            );
            assert_eq!(host.calls.len(), 2);
            let mut host = new_host(parent);
            assert!(schema()
                .prepare_document(source(root.into_bytes()), "entry.yaml".into(), &mut host)
                .is_ok());
        }

        // Retained-arena oracles replacing the retired Python JSON graph consumers.
        // Expected resolved YAML and CSV remain the original committed authored bytes.
        use yamaa_adapters::yaml_decode::decode_yaml;
        use yamaa_core::schema::{Document, DocumentNode as N, SchemaSource};

        fn tree(document: &Document, node: usize) -> Value {
            match &document.nodes()[node] {
                N::Null => Value::Null,
                N::Boolean(value) => (*value).into(),
                N::Integer(value) => serde_json::from_str(value).unwrap(),
                N::Float(value) => json!(value),
                N::Text(value) => value.clone().into(),
                N::Sequence(children) => children
                    .iter()
                    .map(|&child| tree(document, child))
                    .collect(),
                N::Mapping(entries) => entries
                    .iter()
                    .map(|&(key, value)| {
                        let N::Text(key) = &document.nodes()[key] else {
                            panic!("independent fixtures use text field names")
                        };
                        (key.clone(), tree(document, value))
                    })
                    .collect::<yamaa_core::json::Map<_, _>>()
                    .into(),
            }
        }
        fn value(document: &Document) -> Value {
            tree(document, document.root())
        }
        fn path(document: &Document, fields: &[&str]) -> usize {
            fields.iter().fold(document.root(), |node, field| {
                document.field(node, field).unwrap()
            })
        }
        fn sequence(document: &Document, node: usize) -> &[usize] {
            let N::Sequence(items) = &document.nodes()[node] else {
                panic!("expected sequence")
            };
            items
        }
        fn window() -> Value {
            json!({
                "schema_version":"1.0", "domain":"OUT", "base":"SRC", "keys":["ID"],
                "input":{"SRC":{"path":"input.csv", "types":{"SEQ":"int","VAL":"int"}}},
                "output":{"path":"out.csv", "columns":["ID","PREV"]},
                "windows":{"VISITS":{"group_by":["G"],"order_by":["SEQ"]}},
                "columns":[
                    {"name":"ID","type":"str","label":"ID","derivation":"SRC.ID"},
                    {"name":"G","type":"str","label":"G","derivation":"SRC.G"},
                    {"name":"SEQ","type":"int","label":"SEQ","derivation":"SRC.SEQ"},
                    {"name":"VAL","type":"int","label":"VAL","derivation":"SRC.VAL"},
                    {"name":"PREV","type":"int","label":"Previous value","derivation":{"row_value":{"source":"VAL","offset":-1,"window":"VISITS"}}}
                ]
            })
        }

        #[derive(Debug, PartialEq)]
        enum Event {
            Resolve(String, String),
            Capture(String, String),
            Rebase(String, String, String),
        }
        #[derive(Default)]
        struct Files {
            events: Vec<Event>,
        }
        impl InheritancePort for Files {
            type Error = std::io::Error;
            fn canonicalize(
                &mut self,
                declaring: &str,
                written: &str,
            ) -> Result<graph::Source, graph::SourceError<Self::Error>> {
                self.events
                    .push(Event::Resolve(declaring.into(), written.into()));
                let candidate = std::path::Path::new(declaring)
                    .parent()
                    .unwrap()
                    .join(written);
                let identity = candidate
                    .canonicalize()
                    .map_err(|_| graph::SourceError::Unavailable)?;
                Ok(graph::Source {
                    identity: identity.to_str().unwrap().into(),
                    display_path: candidate.to_str().unwrap().into(),
                })
            }
            fn capture(
                &mut self,
                source: &graph::Source,
            ) -> Result<Vec<u8>, graph::SourceError<Self::Error>> {
                self.events.push(Event::Capture(
                    source.identity.clone(),
                    source.display_path.clone(),
                ));
                let bytes = std::fs::read(&source.identity).map_err(graph::SourceError::Raised)?;
                Ok(bytes)
            }
            fn rebase(
                &mut self,
                layer: &graph::Source,
                entry: &graph::Source,
                written: &str,
            ) -> Result<String, Self::Error> {
                self.events.push(Event::Rebase(
                    layer.identity.clone(),
                    entry.identity.clone(),
                    written.into(),
                ));
                // All three independent benchmark cases have parents in the entry directory.
                assert_eq!(
                    std::path::Path::new(&layer.identity).parent(),
                    std::path::Path::new(&entry.identity).parent()
                );
                Ok(written.into())
            }
        }
        struct CaseFiles {
            root: std::path::PathBuf,
        }
        impl CaseFiles {
            fn new(case: &Value) -> Self {
                static NEXT: std::sync::atomic::AtomicUsize =
                    std::sync::atomic::AtomicUsize::new(0);
                let root = std::env::temp_dir().join(format!(
                    "yamaa-inheritance-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ));
                std::fs::create_dir(&root).unwrap();
                let root = root.canonicalize().unwrap();
                for source in case["sources"].as_array().unwrap() {
                    let name = source["name"].as_str().unwrap();
                    assert_eq!(std::path::Path::new(name).components().count(), 1);
                    std::fs::write(root.join(name), source["text"].as_str().unwrap().as_bytes())
                        .unwrap();
                }
                Self { root }
            }
        }
        impl Drop for CaseFiles {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.root);
            }
        }
        #[test]
        fn three_complete_committed_resolved_documents_survive_native_preparation() {
            let fixture: Value = serde_json::from_str(include_str!(
                "fixtures/inheritance_preparation_reference.json"
            ))
            .unwrap();
            assert_eq!(fixture["version"], "1");
            assert_eq!(fixture["cases"].as_array().unwrap().len(), 3);
            for case in fixture["cases"].as_array().unwrap() {
                let name = case["name"].as_str().unwrap();
                let materialized = CaseFiles::new(case);
                let root = &materialized.root;
                let entry = root.join("spec_study.yaml");
                let source = Source {
                    identity: entry.to_str().unwrap().into(),
                    bytes: std::fs::read(&entry).unwrap(),
                };
                let entry_pointer = source.bytes.as_ptr();
                let mut files = Files::default();
                let prepared =
                    shipped_schema::prepare(source, entry.to_str().unwrap().into(), &mut files)
                        .unwrap();
                assert_eq!(prepared.source().bytes.as_ptr(), entry_pointer);
                let expected = decode_yaml(case["resolved"].as_str().unwrap().as_bytes()).unwrap();
                let mut expected = value(&expected.document);
                // The committed resolved documents precede the separately specified
                // REQ-1158 and REQ-0780 defaults; descriptor authority is pinned below.
                for declaration in expected["input"].as_object_mut().unwrap().values_mut() {
                    declaration["empty_string"] = "missing".into();
                    declaration["relative_to"] = "declaring".into();
                }
                expected["output"]["relative_to"] = "declaring".into();
                assert_eq!(value(prepared.model().document()), expected, "{name}");
                assert!(prepared
                    .model()
                    .document()
                    .field(prepared.model().document().root(), "parents")
                    .is_none());
                let parent_names: &[&str] = if name == "schema-inherited-output" {
                    &["spec_shared.yaml"]
                } else {
                    &["spec_organization.yaml", "spec_compound.yaml"]
                };
                let expected_parents: Vec<_> = parent_names
                    .iter()
                    .map(|name| root.join(name).to_str().unwrap().to_owned())
                    .collect();
                assert_eq!(
                    prepared
                        .parents()
                        .iter()
                        .map(|parent| &parent.source().identity)
                        .collect::<Vec<_>>(),
                    expected_parents.iter().collect::<Vec<_>>()
                );
                let mut expected_layers = expected_parents.clone();
                expected_layers.push(entry.to_str().unwrap().into());
                assert_eq!(
                    prepared
                        .inheritance()
                        .unwrap()
                        .layers()
                        .iter()
                        .map(|layer| &layer.source.identity)
                        .collect::<Vec<_>>(),
                    expected_layers.iter().collect::<Vec<_>>()
                );
                let mut tape = Vec::new();
                for (index, parent) in prepared.parents().iter().enumerate() {
                    let parent_name = parent_names[index];
                    tape.push(Event::Resolve(
                        entry.to_str().unwrap().into(),
                        parent_name.into(),
                    ));
                    tape.push(Event::Capture(
                        parent.source().identity.clone(),
                        parent.source().identity.clone(),
                    ));
                    let original = case["sources"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|source| source["name"] == parent_name)
                        .unwrap()["text"]
                        .as_str()
                        .unwrap();
                    assert_eq!(parent.source().bytes, original.as_bytes());
                    assert_eq!(
                        parent.raw().locations.len(),
                        parent.raw().document.nodes().len()
                    );
                }
                if name != "schema-inherited-output" {
                    tape.push(Event::Resolve(
                        root.join("spec_compound.yaml").to_str().unwrap().into(),
                        "spec_organization.yaml".into(),
                    ));
                }
                let rebases: &[(&str, &str)] = match name {
                    "schema-inheritance" => &[
                        ("spec_organization.yaml", "input/lb.csv"),
                        ("spec_organization.yaml", "input/not-used.csv"),
                        ("spec_study.yaml", "adlb.csv"),
                    ],
                    "schema-column-composition" => &[
                        ("spec_organization.yaml", "input/lb.csv"),
                        ("spec_study.yaml", "adlb.csv"),
                    ],
                    "schema-inherited-output" => &[
                        ("spec_shared.yaml", "adsl.csv"),
                        ("spec_study.yaml", "input/dm.csv"),
                    ],
                    _ => panic!("closed independent case inventory"),
                };
                for &(layer, written) in rebases {
                    tape.push(Event::Rebase(
                        root.join(layer).to_str().unwrap().into(),
                        entry.to_str().unwrap().into(),
                        written.into(),
                    ));
                }
                assert_eq!(files.events, tape, "{name}");
                let N::Mapping(inputs) = &prepared.model().document().nodes()
                    [path(prepared.model().document(), &["input"])]
                else {
                    panic!()
                };
                for (key, _) in inputs {
                    let N::Text(dataset) = &prepared.model().document().nodes()[*key] else {
                        panic!()
                    };
                    let written = prepared.written_input_origin(dataset, "path").unwrap();
                    assert_eq!(
                        written.written,
                        format!("input/{}.csv", dataset.to_ascii_lowercase())
                    );
                    let declaring = if name == "schema-inherited-output" {
                        &entry
                    } else {
                        &root.join("spec_organization.yaml")
                    };
                    assert_eq!(written.declaring_source, declaring.to_str().unwrap());
                }
                // A held result is independent from the temporary physical fixture.
                // A new attempt must refuse the now-missing parent, never consult an
                // ambient repository/benchmark path or silently reuse an earlier result.
                std::fs::remove_file(root.join(parent_names[0])).unwrap();
                assert_eq!(value(prepared.model().document()), expected);
                let mut missing = Files::default();
                let error = shipped_schema::prepare(
                    Source {
                        identity: entry.to_str().unwrap().into(),
                        bytes: prepared.source().bytes.clone(),
                    },
                    entry.to_str().unwrap().into(),
                    &mut missing,
                )
                .unwrap_err();
                let report = specification_diagnostics::inheritance_failure(error).unwrap();
                assert_eq!(
                    serde_json::from_str::<Value>(&report).unwrap(),
                    json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":[{"phase":"validation","condition":"parent_not_found","requirement":"REQ-0654","spec_paths":["parents"],"context":{"path":parent_names[0],"source":entry.to_str().unwrap()}}]}})
                );
                assert_eq!(
                    missing.events,
                    [Event::Resolve(
                        entry.to_str().unwrap().into(),
                        parent_names[0].into()
                    )]
                );
            }
        }

        #[test]
        fn complete_window_model_keeps_parent_definition_child_override_and_schema_defaults() {
            let mut parent = window();
            parent["windows"]["UNUSED"] = json!({"group_by":["NOT_A_COLUMN"]});
            let child = json!({"schema_version":"1.0","parents":"parent.yaml","windows":{"VISITS":{"order_by":[{"variable":"SEQ","direction":"desc"}]}}});
            let mut host = new_host(parent.to_string().as_bytes());
            let prepared = shipped_schema::prepare(
                source(child.to_string().into_bytes()),
                "entry.yaml".into(),
                &mut host,
            )
            .unwrap_or_else(|_| panic!("valid independent window"));
            let expected = json!({
                "schema_version":"1.0","domain":"OUT","base":"SRC","keys":["ID"],
                "input":{"SRC":{"path":"input.csv","types":{"SEQ":"int","VAL":"int"},"empty_string":"missing","relative_to":"declaring"}},
                "output":{"path":"out.csv","relative_to":"declaring","columns":["ID","PREV"]},
                "columns":[
                    {"name":"ID","type":"str","label":"ID","derivation":{"value":{"source":{"variable":"SRC.ID"}}}},
                    {"name":"SEQ","type":"int","label":"SEQ","derivation":{"value":{"source":{"variable":"SRC.SEQ"}}}},
                    {"name":"VAL","type":"int","label":"VAL","derivation":{"value":{"source":{"variable":"SRC.VAL"}}}},
                    {"name":"PREV","type":"int","label":"Previous value","derivation":{"value":{"row_value":{"source":"VAL","offset":-1,"window":{"order_by":[{"variable":"SEQ","direction":"desc","nulls":"last"}]}}}}}
                ]
            });
            let document = prepared.model().document();
            assert_eq!(value(document), expected);
            let inherited = prepared.inheritance().unwrap();
            assert_eq!(
                inherited
                    .layers()
                    .iter()
                    .map(|layer| layer.source.identity.as_str())
                    .collect::<Vec<_>>(),
                ["parent.yaml", "entry.yaml"]
            );
            assert!(inherited.provenance().iter().any(|origin| origin.path
                == "columns.PREV.derivation.value.row_value.window"
                && origin.layer == 0));
            assert!(inherited
                .provenance()
                .iter()
                .any(|origin| origin.path == "windows.VISITS.order_by" && origin.layer == 1));
            let previous = *sequence(document, path(document, &["columns"]))
                .last()
                .unwrap();
            let derivation = document.field(previous, "derivation").unwrap();
            let value_node = document.field(derivation, "value").unwrap();
            let expression = document.field(value_node, "row_value").unwrap();
            let window = document.field(expression, "window").unwrap();
            let order = document.field(window, "order_by").unwrap();
            let term = sequence(document, order)[0];
            let nulls = document.field(term, "nulls").unwrap();
            // Earlier layer materialization has already supplied nulls. Preserve the
            // actual normalization-input authority; do not infer a Default descriptor
            // origin from the equal text in a different schema arena.
            assert_eq!(prepared.origins()[nulls].source, SchemaSource::Input);
            assert_eq!(
                prepared.normalization_input().nodes()[prepared.origins()[nulls].node],
                N::Text("last".into())
            );
            assert_eq!(
                prepared.declaring_source("windows.VISITS.order_by[0].nulls"),
                Some("entry.yaml")
            );
            assert_eq!(
                host.calls
                    .iter()
                    .filter(|call| call.starts_with("capture:"))
                    .count(),
                1
            );
        }

        #[test]
        fn count_dependency_pruning_keeps_complete_normalized_application_model() {
            let authored = json!({
                "schema_version":"1.0","parents":[],"domain":"OUT","keys":["ID"],
                "input":{"DM":"dm.csv","EX":"ex.csv","DEAD":"never-read.csv"}, "base":"DM",
                "output":{"path":"out.csv","columns":["ID","N"]},
                "columns":[
                    {"name":"ID","type":"str","derivation":"DM.ID"},
                    {"name":"N","type":"int","derivation":{"aggregate":{"expr":"COUNT(EX.*)"}}},
                    {"name":"UNUSED","type":"int","derivation":{"aggregate":{"expr":"COUNT(DEAD.*)"}}}
                ]
            });
            let mut host = new_host(b"unused");
            let prepared = shipped_schema::prepare(
                source(authored.to_string().into_bytes()),
                "entry.yaml".into(),
                &mut host,
            )
            .unwrap_or_else(|_| panic!("valid normalized count scope"));
            assert_eq!(
                value(prepared.model().document()),
                json!({
                    "schema_version":"1.0","domain":"OUT","keys":["ID"],
                    "input":{"DM":{"path":"dm.csv","empty_string":"missing","relative_to":"declaring"},"EX":{"path":"ex.csv","empty_string":"missing","relative_to":"declaring"}}, "base":"DM",
                    "output":{"path":"out.csv","relative_to":"declaring","columns":["ID","N"]},
                    "columns":[
                        {"name":"ID","type":"str","derivation":{"value":{"source":{"variable":"DM.ID"}}}},
                        {"name":"N","type":"int","derivation":{"value":{"aggregate":{"expr":"COUNT(EX.*)"}}}}
                    ]
                })
            );
            assert_eq!(
                host.calls,
                [
                    "rebase:entry.yaml:entry.yaml:never-read.csv",
                    "rebase:entry.yaml:entry.yaml:dm.csv",
                    "rebase:entry.yaml:entry.yaml:ex.csv",
                    "rebase:entry.yaml:entry.yaml:out.csv"
                ]
            );
            assert_eq!(prepared.written_source_path("DM"), Some("dm.csv"));
            assert_eq!(prepared.written_source_path("EX"), Some("ex.csv"));
        }

        #[test]
        fn explicit_null_handler_and_raw_origin_remain_distinct_from_inherited_clear() {
            let mut authored = window();
            authored["intermediates"] =
                json!([{"id":"LOOK","dataset":"SRC","key":["ID"],"no_match":null}]);
            let source = source(authored.to_string().into_bytes());
            let raw_pointer = source.bytes.as_ptr();
            let mut host = new_host(b"unused");
            let prepared = shipped_schema::prepare(source, "entry.yaml".into(), &mut host)
                .unwrap_or_else(|_| panic!("standalone literal null"));
            assert_eq!(prepared.source().bytes.as_ptr(), raw_pointer);
            assert!(host.calls.is_empty());
            let document = prepared.model().document();
            assert_eq!(
                value(document),
                json!({
                    "schema_version":"1.0","domain":"OUT","base":"SRC","keys":["ID"],
                    "input":{"SRC":{"path":"input.csv","types":{"SEQ":"int","VAL":"int"},"empty_string":"missing","relative_to":"declaring"}},
                    "output":{"path":"out.csv","relative_to":"declaring","columns":["ID","PREV"]},
                    "columns":[
                        {"name":"ID","type":"str","label":"ID","derivation":{"value":{"source":{"variable":"SRC.ID"}}}},
                        {"name":"G","type":"str","label":"G","derivation":{"value":{"source":{"variable":"SRC.G"}}}},
                        {"name":"SEQ","type":"int","label":"SEQ","derivation":{"value":{"source":{"variable":"SRC.SEQ"}}}},
                        {"name":"VAL","type":"int","label":"VAL","derivation":{"value":{"source":{"variable":"SRC.VAL"}}}},
                        {"name":"PREV","type":"int","label":"Previous value","derivation":{"value":{"row_value":{"source":"VAL","offset":-1,"window":{"group_by":["G"],"order_by":[{"variable":"SEQ","direction":"asc","nulls":"last"}]}}}}}
                    ],
                    "intermediates":[{"id":"LOOK","dataset":"SRC","key":["ID"],"no_match":null}]
                })
            );
            let intermediate = sequence(document, path(document, &["intermediates"]))[0];
            let null = document.field(intermediate, "no_match").unwrap();
            assert_eq!(document.nodes()[null], N::Null);
            let raw = &prepared.raw().document;
            let raw_intermediate = sequence(raw, path(raw, &["intermediates"]))[0];
            assert_eq!(prepared.origins()[null].source, SchemaSource::Input);
            assert_eq!(
                prepared.origins()[null].node,
                raw.field(raw_intermediate, "no_match").unwrap()
            );
            assert!(!prepared.origins()[null].generated);
            assert_eq!(
                prepared.declaring_source("intermediates.LOOK.no_match"),
                Some("entry.yaml")
            );
            // Standalone expansion supplies the same default from its exact admitted
            // descriptor, unlike the already-materialized inherited input above.
            let previous = *sequence(document, path(document, &["columns"]))
                .last()
                .unwrap();
            let derivation = document.field(previous, "derivation").unwrap();
            let value_node = document.field(derivation, "value").unwrap();
            let expression = document.field(value_node, "row_value").unwrap();
            let window = document.field(expression, "window").unwrap();
            let order = document.field(window, "order_by").unwrap();
            let term = sequence(document, order)[0];
            let nulls = document.field(term, "nulls").unwrap();
            let descriptor = prepared
                .schema()
                .structure()
                .class_named("order_term_class")
                .unwrap()
                .fields
                .iter()
                .find(|field| field.name == "nulls")
                .unwrap()
                .descriptor;
            let admitted = &prepared.schema().structure().descriptors()[descriptor];
            assert_eq!(
                prepared.origins()[nulls].source,
                SchemaSource::Default {
                    module: admitted.module,
                    descriptor
                }
            );
            assert_eq!(
                prepared.origins()[nulls].node,
                admitted.descriptor.default_node().unwrap()
            );
            assert_eq!(
                prepared.schema().structure().modules()[admitted.module]
                    .document
                    .nodes()[prepared.origins()[nulls].node],
                N::Text("last".into())
            );
        }

        #[test]
        fn introducing_null_handler_survives_explicit_composition_and_empty_child() {
            let mut parent = window();
            parent["intermediates"] =
                json!([{"id":"LOOK","dataset":"SRC","key":["ID"],"no_match":null}]);
            parent["columns"][4]["derivation"] = json!("LOOK.VAL");
            for empty_child in [false, true] {
                let mut host = new_host(parent.to_string().as_bytes());
                let authored = if empty_child {
                    json!({"schema_version":"1.0","parents":"parent.yaml"})
                } else {
                    let mut authored = parent.clone();
                    authored["parents"] = json!([]);
                    authored
                };
                let prepared = shipped_schema::prepare(
                    source(authored.to_string().into_bytes()),
                    "entry.yaml".into(),
                    &mut host,
                )
                .unwrap_or_else(|_| panic!("introducing null is a value"));
                let document = prepared.model().document();
                let intermediate = sequence(document, path(document, &["intermediates"]))[0];
                assert_eq!(
                    document.nodes()[document.field(intermediate, "no_match").unwrap()],
                    N::Null
                );
                assert_eq!(
                    prepared.declaring_source("intermediates.LOOK.no_match"),
                    Some(if empty_child {
                        "parent.yaml"
                    } else {
                        "entry.yaml"
                    })
                );
            }
        }

        #[test]
        fn inherited_null_or_literal_handler_is_still_cleared() {
            for inherited in [json!(null), json!("UNKNOWN")] {
                let mut parent = window();
                parent["intermediates"] =
                    json!([{"id":"LOOK","dataset":"SRC","key":["ID"],"no_match":inherited}]);
                parent["columns"][4]["derivation"] = json!("LOOK.VAL");
                let mut host = new_host(parent.to_string().as_bytes());
                let child = json!({"schema_version":"1.0","parents":"parent.yaml","intermediates":[{"id":"LOOK","no_match":null}]});
                let prepared = shipped_schema::prepare(
                    source(child.to_string().into_bytes()),
                    "entry.yaml".into(),
                    &mut host,
                )
                .unwrap_or_else(|_| panic!("valid optional clear"));
                let document = prepared.model().document();
                let intermediate = sequence(document, path(document, &["intermediates"]))[0];
                assert!(document.field(intermediate, "no_match").is_none());
            }
        }

        #[test]
        fn introduced_invalid_nulls_keep_their_schema_constraints() {
            for (field, condition) in [("label", "invalid_field_type"), ("type", "invalid_clear")] {
                let mut authored = window();
                authored["parents"] = json!([]);
                authored["columns"][0][field] = json!(null);
                let mut host = new_host(b"unused");
                let error = shipped_schema::prepare(
                    source(authored.to_string().into_bytes()),
                    "entry.yaml".into(),
                    &mut host,
                )
                .expect_err("invalid column field");
                let report = outcome(error);
                assert_eq!(report["outcome"]["diagnostics"][0]["condition"], condition);
                assert_eq!(
                    report["outcome"]["diagnostics"][0]["spec_paths"][0],
                    format!("columns.ID.{field}")
                );
            }
        }

        #[test]
        fn distinct_written_aliases_keep_first_canonical_arena_and_complete_authored_model() {
            struct Aliases(Host);
            impl InheritancePort for Aliases {
                type Error = Payload;
                fn canonicalize(
                    &mut self,
                    declaring: &str,
                    written: &str,
                ) -> Result<graph::Source, graph::SourceError<Self::Error>> {
                    self.0.calls.push(format!("resolve:{declaring}:{written}"));
                    assert!(["base.yaml", "alias.yaml"].contains(&written));
                    Ok(graph::Source {
                        identity: "canonical/base.yaml".into(),
                        display_path: format!("written/{written}"),
                    })
                }
                fn capture(
                    &mut self,
                    source: &graph::Source,
                ) -> Result<Vec<u8>, graph::SourceError<Self::Error>> {
                    self.0.capture(source)
                }
                fn rebase(
                    &mut self,
                    layer: &graph::Source,
                    entry: &graph::Source,
                    written: &str,
                ) -> Result<String, Self::Error> {
                    self.0.rebase(layer, entry, written)
                }
            }
            let original = json!({"schema_version":"1.0","domain":"OUT","keys":["ID"],"input":{"DM":"data.csv"},"base":"DM","output":{"path":"out.csv","columns":["ID"]},"columns":[{"name":"ID","type":"str","derivation":"DM.ID"}]}).to_string().into_bytes();
            let pointer = original.as_ptr();
            let entry = b"schema_version: '1.0'\nparents: [base.yaml, alias.yaml]\n".to_vec();
            let mut host = Aliases(Host {
                bytes: [("canonical/base.yaml".into(), original)]
                    .into_iter()
                    .collect(),
                calls: vec![],
                failure: None,
                fail_at: usize::MAX,
            });
            let prepared = shipped_schema::prepare(source(entry), "entry.yaml".into(), &mut host)
                .unwrap_or_else(|_| panic!("valid canonical alias"));
            assert_eq!(
                value(prepared.model().document()),
                json!({"schema_version":"1.0","domain":"OUT","keys":["ID"],"input":{"DM":{"path":"data.csv","empty_string":"missing","relative_to":"declaring"}},"base":"DM","output":{"path":"out.csv","relative_to":"declaring","columns":["ID"]},"columns":[{"name":"ID","type":"str","derivation":{"value":{"source":{"variable":"DM.ID"}}}}]})
            );
            assert_eq!(
                host.0.calls,
                [
                    "resolve:entry.yaml:base.yaml".to_owned(),
                    "capture:canonical/base.yaml".to_owned(),
                    "resolve:entry.yaml:alias.yaml".to_owned(),
                    "rebase:canonical/base.yaml:entry.yaml:data.csv".to_owned(),
                    "rebase:canonical/base.yaml:entry.yaml:out.csv".to_owned()
                ]
            );
            assert_eq!(prepared.parents().len(), 1);
            assert_eq!(prepared.parents()[0].source().bytes.as_ptr(), pointer);
            let inherited = prepared.inheritance().unwrap();
            assert_eq!(
                inherited
                    .layers()
                    .iter()
                    .map(|layer| layer.source.identity.as_str())
                    .collect::<Vec<_>>(),
                ["canonical/base.yaml", "entry.yaml"]
            );
            assert_eq!(
                inherited.layers()[0].source.display_path,
                "written/base.yaml"
            );
            let origin = prepared.written_input_origin("DM", "path").unwrap();
            assert_eq!(
                (origin.declaring_source, origin.written),
                ("canonical/base.yaml", "data.csv")
            );
            assert_eq!(
                prepared.declaring_source("input.DM.path"),
                Some("canonical/base.yaml")
            );
        }
        #[test]
        fn empty_child_reports_the_cycle_derivations_of_its_parent_for_negative_benchmarks() {
            // #2293: an inherited dependency_cycle names the cycle's derivation paths,
            // exactly as the flat build of each parent benchmark records in its
            // committed expected/error.yaml, instead of collapsing to ["columns"].
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
            for (name, paths) in [
                (
                    "negative-adlb-computed-param",
                    &[
                        "columns.ALTVAL.derivation.aggregate",
                        "columns.AVAL.derivation.case",
                    ][..],
                ),
                (
                    "negative-first-available-self",
                    &["columns.SEVAL.derivation.first_available"][..],
                ),
                (
                    "negative-row-value-self",
                    &[
                        "columns.AVALF.derivation.first_available",
                        "columns.PRIORF.derivation.row_value",
                    ][..],
                ),
            ] {
                let dir = root.join(name);
                let parent = std::fs::read(dir.join("spec.yaml")).unwrap();
                let expected =
                    String::from_utf8(std::fs::read(dir.join("expected/error.yaml")).unwrap())
                        .unwrap();
                assert!(expected.contains("condition: dependency_cycle"), "{name}");
                assert!(expected.contains("requirement: REQ-0072"), "{name}");
                for path in paths {
                    assert!(
                        expected.contains(path),
                        "{name}: flat expectation pins {path}"
                    );
                }
                let mut host = new_host(&parent);
                let error = shipped_schema::prepare(
                    source(b"schema_version: '1.0'\nparents: parent.yaml\n".to_vec()),
                    "entry.yaml".into(),
                    &mut host,
                )
                .unwrap_err();
                let actual = outcome(error);
                assert_eq!(actual["outcome"]["status"], "invalid", "{name}");
                let diagnostics = actual["outcome"]["diagnostics"].as_array().unwrap();
                assert_eq!(diagnostics.len(), 1, "{name}: {diagnostics:?}");
                let diagnostic = &diagnostics[0];
                assert_eq!(diagnostic["phase"], "validation", "{name}");
                assert_eq!(diagnostic["condition"], "dependency_cycle", "{name}");
                assert_eq!(diagnostic["requirement"], "REQ-0072", "{name}");
                assert_eq!(diagnostic["spec_paths"], json!(paths), "{name}");
            }
        }
    }
    mod odm_declaration_fixture {
        use crate::support::odm_declaration as fixture;
        use yamaa_core::json::{json, Value};

        fn spec(derivation: Value) -> Value {
            json!({"schema_version":"1.0", "domain":"RESULT", "keys":["USUBJID"],
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
            fixture::observe(&spec.to_string(), headers)
        }

        #[test]
        fn canonical_catalog_excludes_vendor_fields_and_header_case_is_native_owned() {
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

        #[test]
        fn overridden_partial_column_preserves_scope_fact_and_unrelated_binding_refusal() {
            let mut source = spec(json!({"odm":"ODM.IT.DM.AGE"}));
            source["input"]["DM"] = json!("never-read-other.csv");
            source["rows"] =
                json!([{"id":"other_source", "dataset":"DM", "derivations":{"AGE":{"literal":0}}}]);
            // yamaa_core::json's authored fixture text orders input DM before ODM.
            let observed = observe(source.clone(), &[vec!["USUBJID".into()], header()]);
            assert_eq!(observed["status"], "prepared");
            assert_eq!(
                observed["binding"]["issues"][0]["condition"],
                "no_applicable_keys"
            );
            source["rows"][0]["derivations"] = json!({});
            let observed = observe(source, &[vec!["USUBJID".into()], header()]);
            assert_eq!(observed["issues"][0]["condition"], "invalid_odm_context");
            assert_eq!(
                observed["issues"][0]["context"],
                json!({"dataset":"ODM","location":"column"})
            );
            assert_eq!(
                observed["issues"][0]["spec_paths"],
                json!(["columns.AGE.derivation.odm"])
            );
        }

        #[test]
        fn intrinsic_vendor_filter_and_intermediate_derive_contexts_precede_metadata() {
            let observed = observe(
                spec(json!({"odm":{"item":"ODM.IT.DM.AGE","filter":"ODM.SITEID = '9'"}})),
                &[],
            );
            assert_eq!(observed["issues"][0]["condition"], "unknown_field");
            assert_eq!(
                observed["issues"][0]["context"],
                json!({"identifier":"ODM.SITEID","dataset":"ODM"})
            );
            assert_eq!(
                observed["issues"][0]["spec_paths"],
                json!(["columns.AGE.derivation.odm.filter"])
            );
            let mut source = spec(json!({"literal":1}));
            source["intermediates"] = json!([{"id":"AGES","dataset":"ODM","derivations":{"AGE":{"odm":"ODM.IT.DM.AGE"}}}]);
            let observed = observe(source, &[]);
            assert_eq!(observed["issues"][0]["condition"], "invalid_odm_context");
            assert_eq!(
                observed["issues"][0]["context"],
                json!({"dataset":"ODM","location":"intermediate"})
            );
            assert_eq!(
                observed["issues"][0]["spec_paths"],
                json!(["intermediates[0].derivations.AGE.odm"])
            );
            let observed = observe(
                spec(
                    json!({"aggregate":{"derive":[{"name":"VALUE","type":"int","derivation":{"odm":"ODM.IT.DM.AGE"}}],"expr":"SUM(VALUE)"}}),
                ),
                &[],
            );
            assert_eq!(observed["issues"][0]["condition"], "invalid_odm_context");
            assert_eq!(
                observed["issues"][0]["context"],
                json!({"dataset":"ODM","location":"derive"})
            );
            assert_eq!(
                observed["issues"][0]["spec_paths"],
                json!(["columns.AGE.derivation.aggregate.derive[0].derivation.odm"])
            );
        }

        #[test]
        fn missing_fixed_fields_and_declared_types_ordinal_preserve_exact_causes() {
            let missing = header()
                .into_iter()
                .filter(|name| name != "FormOID" && name != "FormRepeatKey")
                .collect::<Vec<_>>();
            let observed = observe(spec(json!({"odm":"ODM.IT.DM.AGE"})), &[missing]);
            assert_eq!(
                observed["issues"][0]["condition"],
                "odm_schema_field_missing"
            );
            assert_eq!(
                observed["issues"][0]["context"],
                json!({"dataset":"ODM","fields":["FormOID","FormRepeatKey"]})
            );
            for (field, value) in [
                ("types", json!({"Value":"int"})),
                ("ordinal", json!("SourceOrdinal")),
            ] {
                let mut source = spec(json!({"odm":"ODM.IT.DM.AGE"}));
                source["input"]["ODM"] = json!({"path":"never-read.csv",field:value});
                let observed = observe(source, &[]);
                assert_eq!(observed["issues"][0]["condition"], "odm_schema_field_type");
                assert_eq!(
                    observed["issues"][0]["context"],
                    json!({"dataset":"ODM","declared":field})
                );
                assert_eq!(
                    observed["issues"][0]["spec_paths"],
                    json!([format!("input.ODM.{field}")])
                );
            }
        }
    }
    mod original_intermediate_verifications {
        //! Shipped-schema successors for specification/test_intermediate_verification.py
        //! retired in #2110. Original valid and invalid verification payloads are kept;
        //! private Python model instances are replaced by the native check boundary.
        use std::{
            fs,
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };
        use yamaa_adapters::{file_application, issue_rows::Issue, specification_report::Identity};
        use yamaa_core::domain_entry::Request;
        use yamaa_core::json::{json, Value};

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        struct Study(PathBuf);
        impl Study {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!(
                    "yamaa-original-intermediate-verification-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(path)
            }
            fn check(&self, intermediate: Value) -> Vec<Issue> {
                let document = json!({
                    "schema_version":"1.0", "domain":"OUT", "base":"DS",
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
                fs::write(&entry, serde_json::to_vec(&document).unwrap()).unwrap();
                let entry = entry.to_str().unwrap();
                let issues = file_application::check(
                    Request {
                        specification: entry,
                        environment: None,
                    },
                    Identity {
                        runtime: "test",
                        runtime_version: "test",
                        engine_version: yamaa_core::VERSION,
                        example: "original-intermediate-verification",
                        specification: entry,
                        base_directory: ".",
                    },
                );
                assert!(!self.0.join("never-read.csv").exists());
                assert!(!self.0.join("out.csv").exists());
                issues
            }
        }
        impl Drop for Study {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }

        #[test]
        fn original_short_and_named_intermediate_unique_checks_are_admitted() {
            let study = Study::new();
            let intermediate = json!({
                "id":"DS_EOS", "dataset":"DS",
                "verifications":[
                    {"unique":["STUDYID","USUBJID"]},
                    {"unique":{"id":"one-row-per-subject","columns":["USUBJID"]}}
                ]
            });
            assert!(study.check(intermediate).is_empty());
        }

        #[test]
        fn all_original_invalid_intermediate_unique_payloads_are_refused() {
            let study = Study::new();
            for verification in [
                json!({"unique":[]}),
                json!({"unique":{"columns":[]}}),
                json!({"unique":{"columns":["STUDYID"],"severity":"warning"}}),
                json!({"unique":{"id":null,"columns":["STUDYID"]}}),
                json!({"assert":{"require":"TRUE"}}),
            ] {
                let issues = study.check(json!({
                    "id":"DS_EOS", "dataset":"DS", "verifications":[verification]
                }));
                assert!(!issues.is_empty(), "{verification}");
                assert!(
                    issues.iter().all(|issue| issue.phase == "validation"
                        && issue.condition != "resource_path_missing"
                        && issue
                            .spec_paths
                            .iter()
                            .all(|path| path.starts_with("intermediates"))),
                    "{verification}: {issues:?}"
                );
            }
        }

        #[test]
        fn original_singular_intermediate_verification_field_stays_unknown() {
            let study = Study::new();
            let issues = study.check(json!({
                "id":"DS_EOS", "dataset":"DS", "verification":{"unique":["USUBJID"]}
            }));
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].condition, "unknown_field");
            assert_eq!(issues[0].spec_paths, ["intermediates.DS_EOS.verification"]);
        }

        #[test]
        fn original_absent_intermediate_verifications_remain_optional() {
            assert!(Study::new()
                .check(json!({"id":"DS_EOS","dataset":"DS"}))
                .is_empty());
        }
    }
    mod preparation_dispatch {
        //! Existing YAML representation and independent complete outcomes around engine delegation.
        use std::sync::Arc;
        use yamaa_adapters::{
            specification_diagnostics::capture_failure,
            specification_source::{
                CapturedSchema, Error, InheritanceError, InheritancePort, Source,
            },
            yaml_decode::{decode_yaml, DecodeFailure},
        };
        use yamaa_core::inheritance::{Source as Identity, SourceError};
        use yamaa_core::json::{json, Value};
        use yamaa_core::schema::{DocumentNode as N, NormalizationError, SchemaSource};

        const SCHEMA: &str = "version: '1.0'\nroot_class:\n  - schema_version: {type: str}\n  - domain: {type: [str, int], default: DEFAULT}\n  - input: {type: dict}\n  - keys: {type: 'list[str]'}\n  - output: {type: dict}\n  - columns: {type: 'list[column_class]'}\n  - windows: {type: 'dict[str, window_spec]'}\ncolumn_class:\n  - name: {type: str}\n  - type: {type: str}\n  - derivation: {type: handled_expression_class}\nhandled_expression_class:\n  - value: {type: expression}\nexpression: {registry: expressions}\nexpressions:\n  row_number:\n    - window: {type: window_selection}\nwindow_selection: {type: [str, window_spec]}\nwindow_spec:\n  - group_by: {type: 'list[str]'}\n  - order_by: {type: 'list[order_term]'}\norder_term:\n  - variable: {type: str}\n";
        const VALID: &str = "schema_version: '1.0'\ndomain: X\ninput: {}\nkeys: []\noutput: {path: output.csv, columns: []}\ncolumns: []\n";
        fn source(text: &str) -> Source {
            Source {
                identity: "retained/spec.yaml".into(),
                bytes: text.as_bytes().to_vec(),
            }
        }
        fn schema() -> Arc<CapturedSchema> {
            CapturedSchema::admit(
                vec![Source {
                    identity: "schema.yaml".into(),
                    bytes: SCHEMA.as_bytes().to_vec(),
                }],
                0,
            )
            .unwrap()
        }
        fn outcome(error: &Error) -> Value {
            serde_json::from_str(&capture_failure(error)).unwrap()
        }
        fn invalid(diagnostics: Value) -> Value {
            json!({"protocol":"specification/prototype","outcome":{"status":"invalid","diagnostics":diagnostics}})
        }
        struct NoParents;
        impl InheritancePort for NoParents {
            type Error = std::rc::Rc<usize>;
            fn canonicalize(
                &mut self,
                _: &str,
                _: &str,
            ) -> Result<Identity, SourceError<Self::Error>> {
                panic!("standalone path must not canonicalize")
            }
            fn capture(&mut self, _: &Identity) -> Result<Vec<u8>, SourceError<Self::Error>> {
                panic!("standalone path must not capture")
            }
            fn rebase(
                &mut self,
                _: &Identity,
                _: &Identity,
                _: &str,
            ) -> Result<String, Self::Error> {
                panic!("standalone path must not rebase")
            }
        }

        #[test]
        fn adapter_moves_source_allocations_and_retains_original_physical_locations_and_defaults() {
            let text = VALID.replace("domain: X\n", "");
            let entry = source(&text);
            let bytes = entry.bytes.as_ptr();
            let identity = entry.identity.as_ptr();
            let schema = schema();
            let prepared = schema.dispatch(entry).unwrap();
            assert_eq!(prepared.source().bytes.as_ptr(), bytes);
            assert_eq!(prepared.source().identity.as_ptr(), identity);
            assert_eq!(prepared.source().bytes, text.as_bytes());
            let decoded = decode_yaml(text.as_bytes()).unwrap();
            assert_eq!(prepared.raw().document, decoded.document);
            assert_eq!(prepared.raw().locations, decoded.locations);
            assert_eq!(
                prepared.model().document().nodes()[prepared
                    .model()
                    .document()
                    .field(prepared.model().document().root(), "domain")
                    .unwrap()],
                N::Text("DEFAULT".into())
            );
            let descriptor = schema
                .structure()
                .class_named("root_class")
                .unwrap()
                .fields
                .iter()
                .find(|f| f.name == "domain")
                .unwrap()
                .descriptor;
            let default = schema.structure().descriptors()[descriptor]
                .descriptor
                .default_node()
                .unwrap();
            let domain = prepared
                .model()
                .document()
                .field(prepared.model().document().root(), "domain")
                .unwrap();
            assert_eq!(
                prepared.origins()[domain].source,
                SchemaSource::Default {
                    module: 0,
                    descriptor
                }
            );
            assert_eq!(prepared.origins()[domain].node, default);
            assert!(!prepared.origins()[domain].generated);
            assert!(prepared.parents().is_empty());
            assert!(prepared.windows().is_empty());
            assert!(prepared.inheritance().is_none());
        }

        #[test]
        fn both_semantic_passes_keep_source_identity_and_independent_complete_reports() {
            let schema = schema();
            let text = VALID.replace("'1.0'", "92233720368547758081234567890");
            let entry = source(&text);
            let bytes = entry.bytes.as_ptr();
            let identity = entry.identity.as_ptr();
            let error = schema.dispatch(entry).unwrap_err();
            let Error::Findings(captured) = &error else {
                panic!()
            };
            assert_eq!(captured.source().bytes.as_ptr(), bytes);
            assert_eq!(captured.source().identity.as_ptr(), identity);
            let decoded = decode_yaml(text.as_bytes()).unwrap();
            assert_eq!(captured.document(), Some(&decoded.document));
            let expected:Value=serde_json::from_str("{\"phase\":\"validation\",\"condition\":\"schema_version_mismatch\",\"requirement\":\"REQ-0281\",\"spec_paths\":[\"schema_version\"],\"context\":{\"expected\":\"1.0\",\"actual\":92233720368547758081234567890}}").unwrap();
            assert_eq!(outcome(&error), invalid(json!([expected])));
            let text="schema_version: '1.0'\ninput: {}\nkeys: []\noutput: {}\ncolumns:\n  - {name: Z, type: int, derivation: {value: {row_number: {window: SECOND}}}}\n  - {name: A, type: int, derivation: {value: {row_number: {window: FIRST}}}}\n";
            let entry = source(text);
            let bytes = entry.bytes.as_ptr();
            let identity = entry.identity.as_ptr();
            let error = schema.dispatch(entry).unwrap_err();
            let Error::Findings(captured) = &error else {
                panic!()
            };
            assert_eq!(captured.source().bytes.as_ptr(), bytes);
            assert_eq!(captured.source().identity.as_ptr(), identity);
            let normalized = captured.document().unwrap();
            assert_eq!(
                normalized.nodes()[normalized.field(normalized.root(), "domain").unwrap()],
                N::Text("DEFAULT".into())
            );
            assert_ne!(normalized, &decode_yaml(text.as_bytes()).unwrap().document);
            assert_eq!(
                outcome(&error),
                invalid(json!([
                    {"phase":"validation","condition":"unknown_window","requirement":"REQ-1253","spec_paths":["columns.Z.derivation.row_number.window"],"context":{"window":"SECOND"}},
                    {"phase":"validation","condition":"unknown_window","requirement":"REQ-1253","spec_paths":["columns.A.derivation.row_number.window"],"context":{"window":"FIRST"}}
                ]))
            );
        }

        #[test]
        fn final_model_findings_retain_no_pass_document_and_move_original_source() {
            let text = VALID
                .replace("domain: X", "domain: 7")
                .replace("{path: output.csv, columns: []}", "{}");
            let entry = source(&text);
            let pointer = entry.bytes.as_ptr();
            let error = schema().dispatch(entry).unwrap_err();
            let Error::Findings(captured) = &error else {
                panic!()
            };
            assert!(captured.document().is_none());
            assert_eq!(captured.source().bytes.as_ptr(), pointer);
            assert_eq!(
                outcome(&error),
                invalid(json!([
                    {"phase":"validation","condition":"model_contract_mismatch","requirement":"REQ-0287","spec_paths":["domain"],"context":{"reason":"Input should be a valid string"}},
                    {"phase":"validation","condition":"model_contract_mismatch","requirement":"REQ-0285","spec_paths":["output.path"],"context":{"reason":"Field required"}},
                    {"phase":"validation","condition":"model_contract_mismatch","requirement":"REQ-0285","spec_paths":["output.columns"],"context":{"reason":"Field required"}}
                ]))
            );
        }

        // Exercise the live dispatch gate, while preserving its standalone representation.
        trait Dispatch {
            fn dispatch(
                self: &Arc<Self>,
                source: Source,
            ) -> Result<yamaa_adapters::specification_source::PreparedDocument, Error>;
        }
        impl Dispatch for CapturedSchema {
            fn dispatch(
                self: &Arc<Self>,
                source: Source,
            ) -> Result<yamaa_adapters::specification_source::PreparedDocument, Error> {
                match self.prepare_document(source, "retained/spec.yaml".into(), &mut NoParents) {
                    Ok(prepared) => Ok(prepared),
                    Err(InheritanceError::Entry(error)) => Err(error),
                    Err(_) => panic!("standalone dispatch selected inheritance"),
                }
            }
        }
        #[test]
        fn physical_decode_and_source_boundary_precede_lifecycle() {
            let schema = schema();
            assert!(matches!(
                schema.dispatch(source("parents: null\n[unterminated")),
                Err(Error::Decode {
                    error: DecodeFailure::InvalidYaml { .. },
                    ..
                })
            ));
            let mut entry = source("[unterminated");
            entry
                .bytes
                .resize(yamaa_core::limits::Limits::SPEC_BYTES + 1, b'x');
            assert!(matches!(schema.dispatch(entry), Err(Error::Decode {
        error: DecodeFailure::Limit(error), ..
    }) if error.resource == "spec_bytes" && error.limit == yamaa_core::limits::Limits::SPEC_BYTES
        && error.actual == Some(yamaa_core::limits::Limits::SPEC_BYTES + 1)));
        }

        #[test]
        fn invalid_schema_default_precedes_authored_version_through_live_dispatch() {
            let schema = CapturedSchema::admit(
                vec![Source {
                    identity: "schema.yaml".into(),
                    bytes: SCHEMA
                        .replace(
                            "type: [str, int], default: DEFAULT",
                            "type: str, default: 7",
                        )
                        .into_bytes(),
                }],
                0,
            )
            .unwrap();
            let Error::Normalize(NormalizationError::Defaults(defaults)) = schema
                .dispatch(source("schema_version: '99.0'\nwindows: {BAD: broken}\n"))
                .unwrap_err()
            else {
                panic!()
            };
            assert_eq!(defaults.len(), 1);
            assert_eq!(
                defaults[0].diagnostics,
                vec![yamaa_core::schema::SchemaDiagnostic {
                    path: "root_class.domain.default".into(),
                    code: yamaa_core::diagnostic::ConditionCode::SchemaInvalidFieldType,
                    context: vec![
                        (
                            "expected",
                            yamaa_core::schema::SchemaContext::Text("str".into())
                        ),
                        (
                            "actual",
                            yamaa_core::schema::SchemaContext::ValueKind(
                                yamaa_core::schema::DocumentKind::Integer
                            )
                        )
                    ],
                }]
            );
        }

        mod native {
            use super::*;
            use std::{
                fs,
                path::PathBuf,
                sync::atomic::{AtomicUsize, Ordering},
            };
            use yamaa_adapters::{
                file_preparation::{Error as NativeError, FileSpecification},
                file_resources::Resources,
            };
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            struct Study(PathBuf);
            impl Study {
                fn new() -> Self {
                    let path = std::env::temp_dir().join(format!(
                        "yamaa-dispatch-{}-{}",
                        std::process::id(),
                        NEXT.fetch_add(1, Ordering::Relaxed)
                    ));
                    fs::create_dir(&path).unwrap();
                    fs::create_dir(path.join("entry")).unwrap();
                    fs::create_dir(path.join("parent")).unwrap();
                    Self(fs::canonicalize(path).unwrap())
                }
                fn identity(&self, name: &str) -> String {
                    let path = self.0.join(name).to_str().unwrap().to_string();
                    #[cfg(windows)]
                    {
                        path.strip_prefix(r"\\?\")
                            .unwrap_or(&path)
                            .replace('\\', "/")
                    }
                    #[cfg(unix)]
                    {
                        path
                    }
                }
                fn prepare(&self, text: &str) -> Result<FileSpecification, NativeError> {
                    fs::write(self.0.join("entry/root.yaml"), text).unwrap();
                    FileSpecification::prepare(
                        Resources::new(
                            self.0.to_str().unwrap(),
                            self.0.join("entry").to_str().unwrap(),
                            &[],
                        )
                        .unwrap(),
                        "root.yaml",
                    )
                }
                fn report(&self, text: &str) -> Value {
                    let Err(error) = self.prepare(text) else {
                        panic!("native rejection")
                    };
                    serde_json::from_str(&error.into_message()).unwrap()
                }
            }
            impl Drop for Study {
                fn drop(&mut self) {
                    fs::remove_dir_all(&self.0).unwrap();
                }
            }
            const VALID:&str="schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: never-read.csv}\noutput: {path: never-written.csv, columns: [ID]}\ncolumns: [{name: ID, type: int, derivation: SRC.ID}]\n";
            #[test]
            fn native_raw_absent_null_empty_and_scalar_routes_preserve_independent_complete_reports(
            ) {
                let study = Study::new();
                let standalone = study.prepare(VALID).unwrap();
                assert!(standalone.run().document().inheritance().is_none());
                assert_eq!(standalone.capture_reads(), 1);
                let inherited = study.prepare(&format!("{VALID}parents: []\n")).unwrap();
                assert!(inherited.run().document().inheritance().is_some());
                assert_eq!(inherited.capture_reads(), 1);
                for parents in ["null", "7"] {
                    assert_eq!(
                        study.report(&format!("{VALID}parents: {parents}\n")),
                        invalid(json!([{
                            "phase":"validation","condition":"invalid_field_type","requirement":"REQ-0287","spec_paths":["parents"],
                            "context":{"expected":"path","actual":if parents=="null" {json!("null")} else {json!("int")},"source":study.identity("entry/root.yaml"),"entry":study.identity("entry/root.yaml")}
                        }]))
                    );
                }
            }
            #[test]
            fn native_version_missing_and_layer_findings_keep_full_original_identities_and_order() {
                let study = Study::new();
                let root = study.identity("entry/root.yaml");
                assert_eq!(
                    study.report(&format!(
                        "{}parents: absent.yaml\n",
                        VALID.replace("'1.0'", "'99.0'")
                    )),
                    invalid(json!([{
                        "phase":"validation","condition":"schema_version_mismatch","requirement":"REQ-0245","spec_paths":["schema_version"],"context":{"expected":"1.0","actual":"99.0","source":root,"entry":root}
                    }]))
                );
                assert_eq!(
                    study.report(&format!("{VALID}parents: absent.yaml\n")),
                    invalid(json!([{
                        "phase":"validation","condition":"parent_not_found","requirement":"REQ-0654","spec_paths":["parents"],"context":{"path":"absent.yaml","source":root}
                    }]))
                );
                fs::write(
                    study.0.join("parent/base.yaml"),
                    "schema_version: '1.0'\ndomain: bad-name\n",
                )
                .unwrap();
                assert_eq!(
                    study.report(&format!("{VALID}parents: ../parent/base.yaml\n")),
                    invalid(json!([{
                        "phase":"validation","condition":"pattern_mismatch","requirement":"REQ-0287","spec_paths":["domain"],"context":{"value":"bad-name","pattern":"^[A-Za-z_][A-Za-z0-9_]*$","source":study.identity("parent/base.yaml"),"entry":root}
                    }]))
                );
            }
            #[test]
            fn native_diamond_retains_first_capture_order_postorder_and_one_snapshot_per_physical_input(
            ) {
                let study = Study::new();
                fs::write(study.0.join("parent/common.yaml"), VALID).unwrap();
                fs::write(
                    study.0.join("parent/a.yaml"),
                    "schema_version: '1.0'\nparents: [common.yaml]\n",
                )
                .unwrap();
                fs::write(
                    study.0.join("parent/b.yaml"),
                    "schema_version: '1.0'\nparents: [./common.yaml,common.yaml]\n",
                )
                .unwrap();
                let prepared=study.prepare("schema_version: '1.0'\nparents: [../parent/a.yaml, ../parent/b.yaml, ../parent/./a.yaml]\n").unwrap();
                let document = prepared.run().document();
                assert_eq!(
                    document
                        .parents()
                        .iter()
                        .map(|p| p.source().identity.as_str())
                        .collect::<Vec<_>>(),
                    [
                        study.identity("parent/a.yaml"),
                        study.identity("parent/common.yaml"),
                        study.identity("parent/b.yaml")
                    ]
                );
                assert_eq!(
                    document
                        .inheritance()
                        .unwrap()
                        .layers()
                        .iter()
                        .map(|l| l.source.identity.as_str())
                        .collect::<Vec<_>>(),
                    [
                        study.identity("parent/common.yaml"),
                        study.identity("parent/a.yaml"),
                        study.identity("parent/b.yaml"),
                        study.identity("entry/root.yaml")
                    ]
                );
                assert_eq!(prepared.capture_reads(), 4);
                assert_eq!(prepared.run().source().path, "../parent/never-read.csv");
                assert_eq!(
                    yamaa_adapters::specification_check::issues(prepared.run()).unwrap(),
                    "[]"
                );
                assert!(!study.0.join("parent/never-read.csv").exists());
                assert!(!study.0.join("parent/never-written.csv").exists());
            }
        }
    }
}
