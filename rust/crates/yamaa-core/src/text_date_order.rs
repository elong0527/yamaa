//! Text order deciding between collected date texts (CHECK-27).
//!
//! Text order puts a proper prefix first (VALUE-20), so collected date text
//! (TIME-7) recorded to two precisions, such as `2025-03-11` and
//! `2025-03-11T07:30`, orders by its length rather than by the time it
//! names. Fixed-width fields order every other pair of collected date texts
//! whose fields are all known as their dates, so a comparison or an order
//! term between two of them misreads time exactly when one is a proper
//! prefix of the other. An unknown field, written `-`, sorts before every
//! known one, which is no choice between two precisions and is not reported.
//!
//! A run keeps, for each site where text order decided such a pair, the
//! least pair it decided. The least pair is the same whatever order the run
//! compares in and however often it compares one pair, so a run divided into
//! partitions, or one that evaluates a selection again a row at a time,
//! reports what the whole run would.

use std::collections::BTreeMap;

/// The two texts, shorter first, when text order decides between them by
/// the prefix rule and both are collected date text.
pub fn decided<'t>(left: &'t str, right: &'t str) -> Option<(&'t str, &'t str)> {
    let (shorter, longer) = by_length(left, right)?;
    judged(shorter.as_bytes(), longer.as_bytes()).then_some((shorter, longer))
}

/// The two texts, shorter first; none when they are as long.
fn by_length<'t, T: AsRef<[u8]> + ?Sized>(left: &'t T, right: &'t T) -> Option<(&'t T, &'t T)> {
    match left.as_ref().len().cmp(&right.as_ref().len()) {
        std::cmp::Ordering::Less => Some((left, right)),
        std::cmp::Ordering::Greater => Some((right, left)),
        std::cmp::Ordering::Equal => None,
    }
}

/// Whether `longer` begins with `shorter`, both are collected date text, and
/// `longer` is read to a finer precision: more of its fields are known before
/// its first unknown one. Fields that begin with or follow an unknown one
/// add no precision TIME-7 reads, so text order chose no precision there.
fn judged(shorter: &[u8], longer: &[u8]) -> bool {
    use crate::iso8601::{collected_bytes, known_prefix};
    if !longer.starts_with(shorter) {
        return false;
    }
    let (Some(shorter), Some(longer)) = (collected_bytes(shorter), collected_bytes(longer)) else {
        return false;
    };
    known_prefix(&longer).len() > known_prefix(&shorter).len()
}

/// Collected date text is ASCII, so each of its bytes is one character.
fn ascii(bytes: &[u8]) -> String {
    bytes.iter().copied().map(char::from).collect()
}

/// The least decided pair one evaluation has met, shorter text first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Least(Option<(String, String)>);

impl Least {
    /// Keep a decided pair when it is the least met so far.
    pub fn offer(&mut self, shorter: &str, longer: &str) {
        let less = self
            .0
            .as_ref()
            .is_none_or(|(least, most)| (shorter, longer) < (least.as_str(), most.as_str()));
        if less {
            self.0 = Some((shorter.to_owned(), longer.to_owned()));
        }
    }

    /// Keep the bytes of `left` and `right` when text order decides between
    /// them by the prefix rule and both are collected date text.
    pub fn offer_bytes(&mut self, left: &[u8], right: &[u8]) {
        if let Some((shorter, longer)) = by_length(left, right) {
            if judged(shorter, longer) {
                self.offer(&ascii(shorter), &ascii(longer));
            }
        }
    }

    /// The least pair met, if any.
    pub fn pair(&self) -> Option<(&str, &str)> {
        self.0
            .as_ref()
            .map(|(shorter, longer)| (shorter.as_str(), longer.as_str()))
    }
}

/// One site where text order decided between two collected date texts, and
/// the least pair it decided there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextDateOrder {
    /// The specification path of the expression or order that compared.
    pub path: String,
    pub shorter: String,
    pub longer: String,
}

impl TextDateOrder {
    /// The warning as a diagnostic record: the condition, the site's path
    /// and the pair, from which [`TextDateOrder::check`] builds the run
    /// report's check row. A run raises it as no issue (CHECK-27).
    pub fn diagnostic(&self) -> crate::diagnostic::Diagnostic {
        use crate::json::Value as Json;
        crate::diagnostic::Diagnostic {
            code: crate::diagnostic::ConditionCode::TextDateOrder,
            spec_paths: vec![self.path.clone()],
            context: [
                ("longer".into(), Json::String(self.longer.clone())),
                ("shorter".into(), Json::String(self.shorter.clone())),
            ]
            .into(),
            source_span: None,
            operand_route: None,
        }
    }

    /// The warning as the run report's check row: the site, named by its
    /// condition, examined as one unit and violated there, with the pair as
    /// the failure's details (CHECK-27).
    pub fn check(&self) -> crate::run_report::Check {
        crate::run_report::Check::warning(self.path.clone(), self.diagnostic())
    }
}

/// The least decided pair of each site, by path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextDateOrders(BTreeMap<String, Least>);

impl TextDateOrders {
    /// Keep a decided pair at `path` when it is the least seen there.
    pub fn record(&mut self, path: &str, shorter: &str, longer: &str) {
        match self.0.get_mut(path) {
            Some(least) => least.offer(shorter, longer),
            None => {
                let mut least = Least::default();
                least.offer(shorter, longer);
                self.0.insert(path.to_owned(), least);
            }
        }
    }

    /// Keep the least pair of each site of either.
    pub fn merge(&mut self, other: Self) {
        for (path, least) in other.0 {
            if let Some((shorter, longer)) = least.pair() {
                self.record(&path, shorter, longer);
            }
        }
    }

    /// Whether no site has met a decided pair.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether the site at `path` has met a decided pair.
    pub fn met(&self, path: &str) -> bool {
        self.0.contains_key(path)
    }

    /// Each site's least pair, in path order.
    pub fn into_orders(self) -> Vec<TextDateOrder> {
        self.0
            .into_iter()
            .filter_map(|(path, least)| {
                let (shorter, longer) = least.0?;
                Some(TextDateOrder {
                    path,
                    shorter,
                    longer,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    // The judgement of a pair, and the least pair a site keeps, apart from
    // any run: the benchmark meets only the pairs its data holds.
    use super::*;

    /// Left, right, and the pair text order decides by length.
    type Case = (
        &'static str,
        &'static str,
        Option<(&'static str, &'static str)>,
    );

    #[test]
    fn text_order_decides_only_a_proper_prefix_between_collected_date_texts() {
        let cases: [Case; 17] = [
            (
                "2025-03-11",
                "2025-03-11T07:30",
                Some(("2025-03-11", "2025-03-11T07:30")),
            ),
            (
                "2025-03-11T08:00:00",
                "2025-03-11T08:00",
                Some(("2025-03-11T08:00", "2025-03-11T08:00:00")),
            ),
            ("2025", "2025-03", Some(("2025", "2025-03"))),
            // Fixed-width fields order a pair that differs inside them.
            ("2025-03-10", "2025-03-11T08:00", None),
            ("2025-03-11T08:00", "2025-03-11T08:00", None),
            // A prefix that is not collected date text, or text that is not.
            ("2025-03-1", "2025-03-11", None),
            ("2025-03-11", "2025-03-11 08:00", None),
            ("Week 1", "Week 10", None),
            ("", "2025", None),
            // An unknown field ends the precision TIME-7 reads, so what
            // begins with it or follows it chooses no precision.
            ("2025", "2025---15", None),
            ("2025-03", "2025-03--T07", None),
            ("2025-03-11", "2025-03-11T-:30", None),
            ("2025---15", "2025---15T07:30", None),
            ("--03-11", "--03-11T07:30", None),
            ("2025-03-11T-", "2025-03-11T-:30", None),
            // Known fields before an unknown one still add precision.
            (
                "2025-03-11",
                "2025-03-11T07:-",
                Some(("2025-03-11", "2025-03-11T07:-")),
            ),
            ("2025", "2025-03--T07", Some(("2025", "2025-03--T07"))),
        ];
        for (left, right, expected) in cases {
            assert_eq!(decided(left, right), expected, "{left} against {right}");
        }
    }

    #[test]
    fn a_site_keeps_its_least_pair_whatever_order_it_meets_them() {
        let pairs = [
            ("2025-03-12", "2025-03-12T10:00"),
            ("2025-03-11T08:00", "2025-03-11T08:00:00"),
            ("2025-03-11", "2025-03-11T07:30"),
        ];
        let least = TextDateOrder {
            path: "intermediates.LAST_DOSE.filter".into(),
            shorter: "2025-03-11".into(),
            longer: "2025-03-11T07:30".into(),
        };
        let mut forward = TextDateOrders::default();
        let mut backward = TextDateOrders::default();
        for (shorter, longer) in pairs {
            forward.record(&least.path, shorter, longer);
        }
        for (shorter, longer) in pairs.iter().rev() {
            backward.record(&least.path, shorter, longer);
        }
        let mut merged = TextDateOrders::default();
        merged.record(&least.path, pairs[0].0, pairs[0].1);
        merged.merge(backward.clone());
        for (label, orders) in [
            ("forward", forward),
            ("backward", backward),
            ("merged", merged),
        ] {
            assert_eq!(orders.into_orders(), vec![least.clone()], "{label}");
        }
    }
}
