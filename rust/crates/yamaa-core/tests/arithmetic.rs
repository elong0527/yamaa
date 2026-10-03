use yamaa_core::numeric::{
    binary, unary, ArithmeticErrorKind, BinaryOperator as B, Number, UnaryOperator as U,
};

fn number(token: &str) -> Number {
    if token == "missing" {
        return Number::Missing;
    }
    let (kind, value) = token.split_once(':').unwrap();
    match kind {
        "int" => Number::Int(value.parse().unwrap()),
        "float" => Number::float(value.parse().unwrap()),
        _ => panic!("unknown fixture type {kind}"),
    }
}

#[test]
fn independently_specified_arithmetic_vectors() {
    for row in include_str!("fixtures/arithmetic.tsv").lines().skip(1) {
        let columns: Vec<_> = row.split('\t').collect();
        assert_eq!(columns.len(), 6, "malformed fixture {row}");
        let (id, expression, operator) = (columns[0], columns[1], columns[2]);
        let (left, right) = (number(columns[3]), number(columns[4]));
        let result = match operator {
            "plus" => unary(U::Plus, left, expression),
            "negate" => unary(U::Negate, left, expression),
            "abs" => unary(U::Abs, left, expression),
            _ => binary(
                match operator {
                    "add" => B::Add,
                    "subtract" => B::Subtract,
                    "multiply" => B::Multiply,
                    "divide" => B::Divide,
                    "modulo" => B::Modulo,
                    _ => panic!("unknown fixture operator {operator}"),
                },
                left,
                right,
                expression,
            ),
        };
        let actual = match result {
            Ok(Number::Missing) => "missing".to_owned(),
            Ok(Number::Int(value)) => format!("int:{value}"),
            Ok(Number::Float(value)) => format!("float:{:016x}", value.get().to_bits()),
            Err(error) => {
                assert_eq!(error.phase(), "derivation", "{id}");
                assert_eq!(error.expression, expression, "{id}");
                let mut text = format!("error:{}:{}", error.condition(), error.requirement());
                if let ArithmeticErrorKind::IntegerOverflow { value } = error.kind {
                    text.push_str(&format!(":{value}"));
                }
                text
            }
        };
        assert_eq!(actual, columns[5], "{id}: {expression}");
    }
}

#[test]
fn every_operator_normalizes_before_the_next_operator() {
    let overflow = binary(B::Multiply, Number::float(1e308), Number::Int(2), "L * 2").unwrap();
    assert_eq!(overflow, Number::Missing);
    assert_eq!(
        binary(B::Divide, overflow, Number::Int(0), "(L * 2) / 0"),
        Ok(Number::Missing)
    );
}

#[test]
fn ordered_float_addition_does_not_reassociate() {
    let first = binary(B::Add, Number::float(0.1), Number::float(0.2), "0.1 + 0.2").unwrap();
    let sum = binary(B::Add, first, Number::float(0.3), "(0.1 + 0.2) + 0.3").unwrap();
    let Number::Float(sum) = sum else {
        panic!("float result required");
    };
    assert_eq!(sum.get().to_bits(), 0x3fe3333333333334);
}

#[test]
fn missing_propagates_for_every_operator_before_arithmetic() {
    for operator in [B::Add, B::Subtract, B::Multiply, B::Divide, B::Modulo] {
        for present in [
            Number::Int(i64::MIN),
            Number::Int(0),
            Number::Int(i64::MAX),
            Number::float(-0.0),
        ] {
            assert_eq!(
                binary(operator, Number::Missing, present, "missing left"),
                Ok(Number::Missing)
            );
            assert_eq!(
                binary(operator, present, Number::Missing, "missing right"),
                Ok(Number::Missing)
            );
        }
    }
    for operator in [U::Plus, U::Negate, U::Abs] {
        assert_eq!(
            unary(operator, Number::Missing, "missing unary"),
            Ok(Number::Missing)
        );
    }
}
