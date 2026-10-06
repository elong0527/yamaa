//! Safe Unicode property lookup for the ECMA RegExpIdentifierName grammar.
use super::identifier_data::{ID_CONTINUE, ID_START};

/// Binary search sorted, disjoint inclusive intervals; no host Unicode tables.
fn member(c: char, ranges: &[(u32, u32)]) -> bool {
    let value = c as u32;
    ranges
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

/// ECMA adds dollar and underscore to Unicode ID_Start.
pub(super) fn start(c: char) -> bool {
    c == '$' || c == '_' || member(c, ID_START)
}

/// ECMA adds dollar, underscore and join controls to Unicode ID_Continue.
pub(super) fn part(c: char) -> bool {
    matches!(c, '$' | '_' | '\u{200c}' | '\u{200d}') || member(c, ID_CONTINUE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    /// Independent bitmap parsing checks every scalar against the authoritative UCD.
    #[test]
    fn every_scalar_matches_pinned_id_properties() {
        assert_eq!(super::super::IDENTIFIER_UNICODE_VERSION, "18.0.0");
        let source = include_str!("../../unicode/18.0.0/DerivedCoreProperties.txt");
        assert!(source.starts_with("# DerivedCoreProperties-18.0.0.txt\n"));
        for (property, ranges, count) in [
            ("ID_Start", ID_START, 158739),
            ("ID_Continue", ID_CONTINUE, 162100),
        ] {
            let mut expected = vec![false; 0x110000];
            for line in source.lines() {
                let data = line.split('#').next().unwrap();
                let Some((interval, name)) = data.split_once(';') else {
                    continue;
                };
                if name.trim() != property {
                    continue;
                }
                let interval = interval.trim();
                let (first, last) = interval.split_once("..").unwrap_or((interval, interval));
                let first = usize::from_str_radix(first, 16).unwrap();
                let last = usize::from_str_radix(last, 16).unwrap();
                for item in &mut expected[first..=last] {
                    assert!(!*item, "overlapping authoritative property range");
                    *item = true;
                }
            }
            assert_eq!(expected.iter().filter(|&&value| value).count(), count);
            for (value, &truth) in expected.iter().enumerate() {
                if let Some(c) = char::from_u32(value as u32) {
                    assert_eq!(member(c, ranges), truth, "{property}: U+{value:04X}");
                    let explicit = matches!(c, '$' | '_')
                        || (property == "ID_Continue" && matches!(c, '\u{200c}' | '\u{200d}'));
                    assert_eq!(
                        if property == "ID_Start" {
                            start(c)
                        } else {
                            part(c)
                        },
                        truth || explicit
                    );
                } else {
                    assert!(!truth);
                }
            }
        }
    }
}
