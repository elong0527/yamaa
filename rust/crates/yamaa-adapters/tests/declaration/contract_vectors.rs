//! Direct native checks of unchanged authored grammar, regex and rounding truth.
use a::Reducer;
use n::NumericFunction;
use serde_json::{json, Value as Json};
use std::collections::BTreeSet;
use yamaa_adapters::yaml_decode::decode_yaml;
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
        p::ParsedKind::And(left, right) => format!("(and {} {})", child(*left), child(*right)),
        p::ParsedKind::Or(left, right) => format!("(or {} {})", child(*left), child(*right)),
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
            Err(n::ParseError::Grammar { failure, .. }) => return Err(failure.condition().into()),
            Err(error) => panic!("grammar vector exceeds resource policy: {error:?}"),
        },
        "aggregate" => match a::parse_aggregate(text) {
            Ok(parsed) => (
                aggregate(&parsed, parsed.root()),
                parsed.identifiers().map(String::from).collect::<Vec<_>>(),
            ),
            Err(a::ParseError::Grammar { failure, .. }) => return Err(failure.condition().into()),
            Err(error) => panic!("grammar vector exceeds resource policy: {error:?}"),
        },
        "predicate" => match p::parse_predicate(text) {
            Ok(parsed) => (
                predicate(&parsed, parsed.root()),
                parsed.identifiers().map(String::from).collect::<Vec<_>>(),
            ),
            Err(p::ParseError::Grammar { failure, .. }) => return Err(failure.condition().into()),
            Err(error) => panic!("grammar vector exceeds resource policy: {error:?}"),
        },
        "string-template" => {
            let parsed =
                StringTemplate::new(text.into(), "columns.RESULT.str_template".into(), None)
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
            for (name, arity) in numeric_doc["vocabulary"]["function"].as_object().unwrap() {
                let lower = arity["min_arguments"].as_u64().unwrap() as usize;
                let upper = arity["max_arguments"].as_u64().map(|value| value as usize);
                let mut counts = BTreeSet::from([0, lower - 1, lower, lower + 1, 8]);
                if let Some(upper) = upper {
                    counts.insert(upper + 1);
                }
                for count in counts {
                    let accepted = count >= lower && upper.is_none_or(|upper| count <= upper);
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
            for (name, declaration) in document["vocabulary"]["reducer"].as_object().unwrap() {
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
    for line in include_str!("../fixtures/regex_contract.tsv")
        .lines()
        .skip(1)
    {
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
        0x9, 0xa, 0xb, 0xc, 0xd, 0x20, 0xa0, 0x1680, 0x2000, 0x2001, 0x2002, 0x2003, 0x2004,
        0x2005, 0x2006, 0x2007, 0x2008, 0x2009, 0x200a, 0x2028, 0x2029, 0x202f, 0x205f, 0x3000,
        0xfeff,
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
                    let captured = "a".repeat(if lazy { minimum } else { maximum.min(length) });
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
    for line in include_str!("../fixtures/rounding_rational.tsv")
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
        serde_json::from_str(include_str!("../fixtures/syntax_controls.json")).unwrap();
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
    for line in include_str!("../fixtures/syntax_robustness.tsv")
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
            "AND", "BETWEEN", "DATE", "DATETIME", "ESCAPE", "FALSE", "IN", "IS", "LIKE", "NOT",
            "NULL", "OR", "TRUE"
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
            let actual = syntax_outcome(family, &format!("A {} B", word.to_ascii_lowercase()));
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
