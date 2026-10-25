//! The condition catalogue (#3700): every condition the engine may report,
//! and every condition a release retired, as one table that each host and
//! the command line give alike.
//!
//! [`conditions`] gives one [`Condition`] per row, sorted by name, with the
//! [`CONDITION_COLUMNS`]: the condition, the severity of its issues, the
//! phases that may report it, the rules its issues cite, its message
//! template, the name a release renamed it from and the release that retired
//! it. `yaml/conditions.yaml`, which the engine bundles when it is built,
//! gives the phases, the template and the history, and the core registry
//! the severity and the rules: the card IDs that the registry's causes of
//! the condition cite, which are the values an issue of the condition can
//! hold in its `rule` column. Nothing is read at run time, so every
//! installation of a release gives the same catalogue, the one
//! `yaml/conditions.yaml` holds at the release's tag.
//!
//! A program written for an earlier release finds each name it matched: a
//! condition that a release renamed names its former name in
//! `renamed_from`, and a condition that a release retired keeps its row,
//! with that release in `retired_in`, no phase, no rule and no message.

use std::collections::BTreeMap;

use crate::diagnostic::{ContextValue, Phase, Severity};
use crate::table::Table;

mod registry;

/// A condition of the registry, as an issue's `condition` column names it:
/// one `yaml/conditions.yaml` holds, among its registered conditions or the
/// conditions a release retired. [`ConditionCode::parse`] refuses any other
/// name, so a code always names a condition the catalogue lists. A
/// [`Diagnostic`](crate::Diagnostic) names a registered one; only a retired
/// row of the catalogue names a retired one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConditionCode {
    name: &'static str,
}

impl ConditionCode {
    /// The condition named `name`, or none for a name the registry holds
    /// neither among its registered conditions nor among those a release
    /// retired.
    pub fn parse(name: &str) -> Option<Self> {
        // Both tables are sorted by condition.
        let registered = registry::REGISTERED
            .binary_search_by(|(condition, ..)| (*condition).cmp(name))
            .map(|found| registry::REGISTERED[found].0);
        let retired = || {
            registry::RETIRED
                .binary_search_by(|(condition, _)| (*condition).cmp(name))
                .map(|found| registry::RETIRED[found].0)
        };
        registered
            .or_else(|_| retired())
            .ok()
            .map(|name| Self { name })
    }

    /// The condition's name, as the issues table's `condition` column and
    /// the catalogue write it.
    pub fn as_str(self) -> &'static str {
        self.name
    }
}

impl std::fmt::Display for ConditionCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

/// The condition catalogue's columns, in order.
pub const CONDITION_COLUMNS: [&str; 7] = [
    "condition",
    "severity",
    "phases",
    "rules",
    "message",
    "renamed_from",
    "retired_in",
];

/// One condition of the catalogue: one row of its table.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Condition {
    /// The condition, as an issue's `condition` column names it.
    pub condition: ConditionCode,
    /// The severity of the condition's issues: error for every condition of
    /// 1.0 (D28), a retired one included.
    pub severity: Severity,
    /// The phases that may report the condition, as `yaml/conditions.yaml`
    /// lists them; none for a retired condition.
    pub phases: Vec<Phase>,
    /// The rules the condition's issues cite: the card IDs the registry's
    /// causes of the condition name, in card order (by prefix, then number).
    /// None for a condition whose issues cite no rule, such as
    /// `engine_rejected`, and for a retired condition.
    pub rules: Vec<&'static str>,
    /// The template the messages of the condition's issues render from
    /// (P-MSG-1); `None` for a retired condition. Not frozen.
    pub message: Option<&'static str>,
    /// The name the condition had before a release renamed it, if one did.
    pub renamed_from: Option<&'static str>,
    /// The release that retired the condition, such as `0.6.0`; `None` for a
    /// condition the engine may report.
    pub retired_in: Option<&'static str>,
}

impl Condition {
    /// The condition as one JSON object with the catalogue's seven keys, as
    /// the command line's `--format json` writes it: its lists as lists, and
    /// a missing cell as `null`.
    pub fn to_value(&self) -> ContextValue {
        let text = |value: &str| ContextValue::Text(value.to_owned());
        let texts = |values: &[&str]| ContextValue::List(values.iter().map(|v| text(v)).collect());
        let optional = |value: Option<&str>| value.map_or(ContextValue::Null, text);
        ContextValue::Map(
            [
                ("condition", text(self.condition.as_str())),
                ("severity", text(self.severity.as_str())),
                ("phases", texts(&self.phase_names())),
                ("rules", texts(&self.rules)),
                ("message", optional(self.message)),
                ("renamed_from", optional(self.renamed_from)),
                ("retired_in", optional(self.retired_in)),
            ]
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
        )
    }

    /// The condition as one line of the command line's table: its seven
    /// cells in column order, separated by a tab, each list's items joined
    /// with `, ` and a missing cell empty.
    pub fn text(&self) -> String {
        [
            self.condition.as_str().to_owned(),
            self.severity.as_str().to_owned(),
            self.phase_names().join(", "),
            self.rules.join(", "),
            self.message.unwrap_or_default().to_owned(),
            self.renamed_from.unwrap_or_default().to_owned(),
            self.retired_in.unwrap_or_default().to_owned(),
        ]
        .join("\t")
    }

    /// The phases as written, in the catalogue's order.
    fn phase_names(&self) -> Vec<&'static str> {
        self.phases.iter().copied().map(Phase::as_str).collect()
    }
}

/// The catalogue [`conditions`] gives: one [`Condition`] per row, sorted by
/// name. The hosts return its [`table`](Conditions::table); the command line
/// prints its [`lines`](Conditions::lines), or with `--format json` each
/// condition's [`to_value`](Condition::to_value).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conditions {
    entries: Vec<Condition>,
}

impl Conditions {
    /// Every condition, sorted by name.
    pub fn entries(&self) -> &[Condition] {
        &self.entries
    }

    /// The catalogue as the command line prints it: a line of the
    /// [`CONDITION_COLUMNS`], then each condition's [`text`](Condition::text),
    /// all separated by tabs.
    pub fn lines(&self) -> Vec<String> {
        std::iter::once(CONDITION_COLUMNS.join("\t"))
            .chain(self.entries.iter().map(Condition::text))
            .collect()
    }

    /// The catalogue's table with the [`CONDITION_COLUMNS`], one row per
    /// condition: `phases` and `rules` are lists of text, and `message`,
    /// `renamed_from` and `retired_in` are missing where a condition has
    /// none.
    pub fn table(&self) -> Table {
        use yamaa_adapters::public_table::{List, PublicColumn, PublicTable};

        let text = |cell: fn(&Condition) -> Option<&str>| {
            PublicColumn::Str(self.entries.iter().map(cell).collect())
        };
        let texts = |cell: fn(&Condition) -> Vec<&str>| {
            PublicColumn::List(Box::new(List::text(
                self.entries.iter().map(|entry| Some(cell(entry))),
            )))
        };
        let columns: [PublicColumn; CONDITION_COLUMNS.len()] = [
            text(|entry| Some(entry.condition.as_str())),
            text(|entry| Some(entry.severity.as_str())),
            texts(Condition::phase_names),
            texts(|entry| entry.rules.clone()),
            text(|entry| entry.message),
            text(|entry| entry.renamed_from),
            text(|entry| entry.retired_in),
        ];
        let columns = CONDITION_COLUMNS
            .into_iter()
            .zip(columns)
            .map(|(name, column)| (name.to_owned(), column))
            .collect();
        Table::from_public(PublicTable::new(columns).expect("the catalogue columns form one table"))
    }
}

/// The condition catalogue of this engine: every condition it may report and
/// every condition a release retired, sorted by name (#3700).
///
/// # Example
///
/// ```
/// let catalogue = yamaa::conditions();
/// let entries = catalogue.entries();
/// assert!(entries.windows(2).all(|pair| pair[0].condition < pair[1].condition));
/// let renamed = entries
///     .iter()
///     .find(|entry| entry.renamed_from == Some("schema_version_mismatch"))
///     .unwrap();
/// assert_eq!(renamed.condition.as_str(), "unsupported_language_version");
/// let retired = entries
///     .iter()
///     .find(|entry| entry.condition.as_str() == "month_required")
///     .unwrap();
/// assert_eq!((retired.retired_in, retired.message), (Some("0.6.0"), None));
/// assert_eq!(catalogue.table().rows(), entries.len());
/// ```
pub fn conditions() -> Conditions {
    // The severity of each condition's issues and the rules they cite, from
    // the registry's causes of the condition.
    let mut causes = BTreeMap::<&'static str, (Severity, Vec<&'static str>)>::new();
    for cause in yamaa_core::diagnostic::CONDITIONS {
        let definition = cause.definition();
        let (_, cited) = causes
            .entry(definition.condition)
            .or_insert_with(|| (crate::diagnostic::from_core(cause.severity()), Vec::new()));
        if let Some(rule) = definition.rule.filter(|rule| !cited.contains(rule)) {
            cited.push(rule);
        }
    }
    let mut entries = registry::REGISTERED
        .iter()
        .map(|&(condition, phases, renamed_from)| {
            // A condition that no cause raises has the registry's one
            // severity, error, and cites no rule.
            let (severity, mut rules) = causes
                .remove(condition)
                .unwrap_or((Severity::Error, Vec::new()));
            rules.sort_by_key(|rule| card_order(rule));
            Condition {
                condition: ConditionCode { name: condition },
                severity,
                phases: phases
                    .iter()
                    .map(|phase| {
                        Phase::parse(phase).expect("the registry names one of the twelve phases")
                    })
                    .collect(),
                rules,
                message: yamaa_core::diagnostic::template(condition),
                renamed_from,
                retired_in: None,
            }
        })
        .chain(
            registry::RETIRED
                .iter()
                .map(|&(condition, release)| Condition {
                    condition: ConditionCode { name: condition },
                    severity: Severity::Error,
                    phases: Vec::new(),
                    rules: Vec::new(),
                    message: None,
                    renamed_from: None,
                    retired_in: Some(release),
                }),
        )
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.condition);
    Conditions { entries }
}

/// A card ID's place in card order: its prefix, then its number, so that
/// `VALUE-7` comes before `VALUE-15`.
fn card_order(rule: &str) -> (&str, u32) {
    let (prefix, number) = rule.rsplit_once('-').unwrap_or((rule, ""));
    (prefix, number.parse().unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`ConditionCode::parse`] searches each registry table by name, so
    /// each is sorted by condition, no name is in both, and every name
    /// parses as itself.
    #[test]
    fn the_registry_tables_are_sorted_disjoint_and_parse() {
        let registered = registry::REGISTERED.iter().map(|(name, ..)| *name);
        let retired = registry::RETIRED.iter().map(|(name, _)| *name);
        for (table, names) in [
            ("registered", registered.collect::<Vec<_>>()),
            ("retired", retired.collect::<Vec<_>>()),
        ] {
            assert!(names.windows(2).all(|pair| pair[0] < pair[1]), "{table}");
            for name in names {
                assert_eq!(
                    ConditionCode::parse(name).map(ConditionCode::as_str),
                    Some(name),
                    "{table}"
                );
            }
        }
        assert!(registry::RETIRED
            .iter()
            .all(|(name, _)| registry::REGISTERED
                .binary_search_by(|(registered, ..)| (*registered).cmp(name))
                .is_err()));
    }
}
