//! Line protocol for checking Rust against the authoritative YAML grammar in CI.
use std::io::{self, BufRead};
use yamaa_core::numeric::{BinaryOperator, UnaryOperator};
use yamaa_core::numeric_parser::{
    parse_numeric, ParseError, ParseLimits, ParsedKind, ParsedNumeric,
};

/// Encode arbitrary UTF-8 text without tabs or line terminators in the protocol.
fn hex(text: &str) -> String {
    text.bytes().map(|byte| format!("{byte:02x}")).collect()
}

/// Render the contract's shape notation; groups affect spans but not this shape.
fn shape(parsed: &ParsedNumeric, id: usize) -> String {
    let node = &parsed.nodes()[id];
    let written = &parsed.expression()[node.span.start..node.span.end];
    match &node.kind {
        ParsedKind::Number { fractional } => {
            format!("({} {written})", if *fractional { "float" } else { "int" })
        }
        ParsedKind::Null => "null".into(),
        ParsedKind::Identifier => format!("(id {written})"),
        ParsedKind::Group { operand } => shape(parsed, *operand),
        ParsedKind::Unary { operator, operand } => {
            let symbol = match operator {
                UnaryOperator::Plus => "pos",
                UnaryOperator::Negate => "neg",
                UnaryOperator::Abs => unreachable!("ABS is a call in syntax"),
            };
            format!("({symbol} {})", shape(parsed, *operand))
        }
        ParsedKind::Binary {
            operator,
            left,
            right,
        } => {
            let symbol = match operator {
                BinaryOperator::Add => "+",
                BinaryOperator::Subtract => "-",
                BinaryOperator::Multiply => "*",
                BinaryOperator::Divide => "/",
                BinaryOperator::Modulo => unreachable!("MOD is a call in syntax"),
            };
            format!(
                "({symbol} {} {})",
                shape(parsed, *left),
                shape(parsed, *right)
            )
        }
        ParsedKind::Call {
            function,
            arguments,
            ..
        } => format!(
            "(call {} {})",
            function.name(),
            arguments
                .iter()
                .map(|&id| shape(parsed, id))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    }
}

/// Replay hex-encoded expressions, one result per input, without modifying fixtures.
fn main() {
    for line in io::stdin().lock().lines() {
        let line = line.expect("read input");
        assert!(
            line.is_ascii() && line.len() % 2 == 0,
            "expected UTF-8 hex input"
        );
        let bytes = (0..line.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&line[i..i + 2], 16).expect("hex byte"))
            .collect();
        let text = String::from_utf8(bytes).expect("UTF-8 source");
        match parse_numeric(&text, ParseLimits::default()) {
            Ok(parsed) => println!(
                "accept\t{}\t{}",
                hex(&shape(&parsed, parsed.root())),
                hex(&parsed.identifiers().collect::<Vec<_>>().join(","))
            ),
            Err(ParseError::Grammar { position, failure }) => println!(
                "reject\t{}\t{}\t{}\t{}",
                failure.condition(),
                failure.requirement(),
                position.byte,
                position.character
            ),
            Err(ParseError::Limit {
                resource, limit, ..
            }) => println!("limit\t{resource:?}\t{limit}"),
        }
    }
}
