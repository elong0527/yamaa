//! Test-only typed replay of the authored shared grammars; no host probe API.
use a::Reducer;
use n::NumericFunction;
use std::io::{self, BufRead};
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

fn hex(text: &str) -> String {
    text.bytes().map(|byte| format!("{byte:02x}")).collect()
}
fn decode(text: &str) -> String {
    assert!(text.is_ascii() && text.len().is_multiple_of(2));
    String::from_utf8(
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect(),
    )
    .unwrap()
}
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
fn accepted(shape: String, identifiers: impl IntoIterator<Item = impl AsRef<str>>) {
    let mut names = identifiers
        .into_iter()
        .map(|s| s.as_ref().to_owned())
        .collect::<Vec<_>>();
    names.sort();
    println!("accept\t{}\t{}", hex(&shape), hex(&names.join(",")));
}
fn main() {
    for line in io::stdin().lock().lines() {
        let line = line.unwrap();
        let (contract, text) = line.split_once('\t').unwrap();
        let text = decode(text);
        match contract {
            "numeric-vocabulary" => accepted(String::new(), numeric_vocabulary()),
            "aggregate-vocabulary" => accepted(String::new(), aggregate_vocabulary()),
            "numeric" => match n::parse_numeric(&text, n::ParseLimits::default()) {
                Ok(parsed) => accepted(numeric(&parsed, parsed.root()), parsed.identifiers()),
                Err(n::ParseError::Grammar { failure, .. }) => {
                    println!("reject\t{}", failure.condition())
                }
                Err(error) => panic!("grammar vector exceeds resource policy: {error:?}"),
            },
            "aggregate" => match a::parse_aggregate(&text, a::ParseLimits::default()) {
                Ok(parsed) => accepted(aggregate(&parsed, parsed.root()), parsed.identifiers()),
                Err(a::ParseError::Grammar { failure, .. }) => {
                    println!("reject\t{}", failure.condition())
                }
                Err(error) => panic!("grammar vector exceeds resource policy: {error:?}"),
            },
            "predicate" => match p::parse_predicate(&text, p::ParseLimits::default()) {
                Ok(parsed) => accepted(predicate(&parsed, parsed.root()), parsed.identifiers()),
                Err(p::ParseError::Grammar { failure, .. }) => {
                    println!("reject\t{}", failure.condition())
                }
                Err(error) => panic!("grammar vector exceeds resource policy: {error:?}"),
            },
            "string-template" => {
                let parsed =
                    StringTemplate::new(text, "columns.RESULT.str_template".into(), None).unwrap();
                if let Some(diagnostic) = parsed.declaration_diagnostic() {
                    println!("reject\t{}", diagnostic.definition().condition);
                } else {
                    accepted(template(&parsed), parsed.identifiers());
                }
            }
            _ => panic!("unknown shared grammar {contract}"),
        }
    }
}
