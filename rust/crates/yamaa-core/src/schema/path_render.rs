//! Bounded Python-style diagnostic labels with an explicit Unicode table.

use super::{Document, DocumentNode as N, ValidationBudget, ValidationError};
use alloc::{format, string::String};

/// Pins label representation independently of a host's Unicode release.
pub const DIAGNOSTIC_UNICODE_VERSION: &str = "18.0.0";

fn printable(c: char) -> bool {
    let value = c as u32;
    super::printable_data::PRINTABLE
        .binary_search_by(|&(start, end)| {
            if end < value {
                core::cmp::Ordering::Less
            } else if start > value {
                core::cmp::Ordering::Greater
            } else {
                core::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

struct Renderer<'a> {
    output: String,
    budget: &'a mut ValidationBudget,
}
impl Renderer<'_> {
    fn append(&mut self, text: &str) -> Result<(), ValidationError> {
        self.budget.work(text.len().saturating_add(1))?;
        self.budget.text(text.len())?;
        self.output.push_str(text);
        Ok(())
    }
    fn quoted(&mut self, text: &str) -> Result<(), ValidationError> {
        self.budget.work(text.len().saturating_mul(2))?;
        let quote = if text.contains('\'') && !text.contains('"') {
            '"'
        } else {
            '\''
        };
        self.append(if quote == '\'' { "'" } else { "\"" })?;
        for c in text.chars() {
            match c {
                '\\' => self.append("\\\\")?,
                '\n' => self.append("\\n")?,
                '\r' => self.append("\\r")?,
                '\t' => self.append("\\t")?,
                c if c == quote => self.append(if quote == '\'' { "\\'" } else { "\\\"" })?,
                c if printable(c) => self.append(c.encode_utf8(&mut [0; 4]))?,
                c => {
                    let value = c as u32;
                    let escaped = if value <= 0xff {
                        format!("\\x{value:02x}")
                    } else if value <= 0xffff {
                        format!("\\u{value:04x}")
                    } else {
                        format!("\\U{value:08x}")
                    };
                    self.append(&escaped)?;
                }
            }
        }
        self.append(if quote == '\'' { "'" } else { "\"" })
    }
    fn value(
        &mut self,
        input: &Document,
        node: usize,
        nested: bool,
        depth: usize,
    ) -> Result<(), ValidationError> {
        self.budget.depth(depth)?;
        self.budget.work(1)?;
        match &input.nodes()[node] {
            N::Text(text) if nested => self.quoted(text),
            N::Text(text) | N::Integer(text) => self.append(text),
            N::Boolean(value) => self.append(if *value { "True" } else { "False" }),
            N::Null => self.append("None"),
            N::Float(value) => self.append(&float_key(*value)),
            N::Sequence(items) => {
                self.append("[")?;
                for (index, &value) in items.iter().enumerate() {
                    if index != 0 {
                        self.append(", ")?;
                    }
                    self.value(input, value, true, depth + 1)?;
                }
                self.append("]")
            }
            N::Mapping(items) => {
                self.append("{")?;
                for (index, &(key, value)) in items.iter().enumerate() {
                    if index != 0 {
                        self.append(", ")?;
                    }
                    self.value(input, key, true, depth + 1)?;
                    self.append(": ")?;
                    self.value(input, value, true, depth + 1)?;
                }
                self.append("}")
            }
        }
    }
}

pub(super) fn label(
    input: &Document,
    node: usize,
    budget: &mut ValidationBudget,
) -> Result<String, ValidationError> {
    let mut renderer = Renderer {
        output: String::new(),
        budget,
    };
    renderer.value(input, node, false, 0)?;
    Ok(renderer.output)
}

/// Python-compatible finite float spelling for diagnostic mapping keys, not value conversion.
fn float_key(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .into();
    }
    let mut buffer = ryu::Buffer::new();
    let text = buffer.format_finite(value.abs());
    let (coefficient, exponent) = text.split_once('e').map_or((text, 0), |(a, b)| {
        (a, b.parse::<i32>().expect("finite exponent"))
    });
    let point = coefficient.find('.').unwrap_or(coefficient.len()) as i32;
    let digits: String = coefficient.chars().filter(|&c| c != '.').collect();
    let leading = digits.bytes().take_while(|&b| b == b'0').count();
    let power = point + exponent - leading as i32 - 1;
    let digits = digits[leading..].trim_end_matches('0');
    let mut result = if value.is_sign_negative() {
        String::from("-")
    } else {
        String::new()
    };
    if (-4..16).contains(&power) {
        let point = power + 1;
        if point <= 0 {
            result.push_str("0.");
            for _ in 0..-point {
                result.push('0');
            }
            result.push_str(digits);
        } else if point as usize >= digits.len() {
            result.push_str(digits);
            for _ in digits.len()..point as usize {
                result.push('0');
            }
            result.push_str(".0");
        } else {
            let point = point as usize;
            result.push_str(&digits[..point]);
            result.push('.');
            result.push_str(&digits[point..]);
        }
    } else {
        result.push_str(&digits[..1]);
        if digits.len() > 1 {
            result.push('.');
            result.push_str(&digits[1..]);
        }
        result.push('e');
        result.push(if power < 0 { '-' } else { '+' });
        result.push_str(&format!("{:02}", power.abs()));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn every_scalar_printability_matches_authoritative_category_annotations() {
        assert_eq!(DIAGNOSTIC_UNICODE_VERSION, "18.0.0");
        let source = include_str!("../../unicode/18.0.0/DerivedCoreProperties.txt");
        let mut expected = vec![false; 0x110000];
        expected[0x20] = true;
        for line in source.lines() {
            let Some((data, comment)) = line.split_once('#') else {
                continue;
            };
            let Some((bounds, property)) = data.split_once(';') else {
                continue;
            };
            if !matches!(property.trim(), "Grapheme_Base" | "Grapheme_Extend") {
                continue;
            }
            let category = comment.split_whitespace().next().unwrap();
            if !matches!(category.as_bytes()[0], b'L' | b'M' | b'N' | b'P' | b'S') {
                continue;
            }
            let mut parts = bounds.trim().split("..");
            let start = u32::from_str_radix(parts.next().unwrap(), 16).unwrap();
            let end = parts
                .next()
                .map_or(start, |part| u32::from_str_radix(part, 16).unwrap());
            for point in start..=end {
                expected[point as usize] = true;
            }
        }
        for (point, &truth) in expected.iter().enumerate() {
            if let Some(c) = char::from_u32(point as u32) {
                assert_eq!(printable(c), truth, "U+{point:04X}");
            }
        }
    }
}
