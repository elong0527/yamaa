//! A source selection over a row's base records that chose nothing
//! (LOOKUP-10).
//!
//! In a specification without `rows`, a `source` binding's `order_by` and
//! `keep` choose among the base records that build a non-key column's row.
//! Whether a row is built from more than one record is a property of the
//! data and the declared keys, so only a run can tell that the selection
//! never decided between two readings: each of its rows held one record, or
//! records that read alike. The value it gave is then each row's own
//! reading, which is seldom what a selection was written for. A successful
//! run reports each such selection as one violated `warning` check of the
//! run report; it fails nothing and changes no value.

use crate::numeric_lifecycle::{HandlerCount, HandlerKind};

/// A selection among the base records of an implicit template's row: the
/// path of the `source` binding that declares it and the field it reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseSelection {
    pub path: String,
    pub identifier: String,
}

impl BaseSelection {
    /// Whether the run's handler counts show that the selection never chose:
    /// its `multiple_matches` count, which counts each row where it decided
    /// between two readings (COLUMN-14), stayed zero. A site the run did not
    /// register reports nothing.
    fn unchosen(&self, counts: &[HandlerCount]) -> bool {
        let site = format!("{}.{}", self.path, HandlerKind::MultipleMatches.name());
        counts
            .iter()
            .find(|count| count.handler == HandlerKind::MultipleMatches && count.spec_path == site)
            .is_some_and(|count| count.count == 0)
    }
}

/// One selection that chose nothing in a successful run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnchosenSelection {
    /// The path of the selection's `keep`.
    pub path: String,
    /// The field the binding reads.
    pub identifier: String,
}

impl UnchosenSelection {
    /// Each selection of `selections` that the run's `counts` show chose
    /// nothing, in path order.
    pub fn of(selections: &[BaseSelection], counts: &[HandlerCount]) -> Vec<Self> {
        let mut unchosen = selections
            .iter()
            .filter(|selection| selection.unchosen(counts))
            .map(|selection| Self {
                path: format!("{}.keep", selection.path),
                identifier: selection.identifier.clone(),
            })
            .collect::<Vec<_>>();
        unchosen.sort_by(|left, right| left.path.cmp(&right.path));
        unchosen.dedup();
        unchosen
    }

    /// The warning as a diagnostic record, from which
    /// [`UnchosenSelection::check`] builds the run report's check row. A run
    /// raises it as no issue (LOOKUP-10).
    pub fn diagnostic(&self) -> crate::diagnostic::Diagnostic {
        crate::diagnostic::Diagnostic::new(
            crate::diagnostic::ConditionCode::UnchosenSelection,
            vec![self.path.clone()],
            [(
                "identifier".into(),
                crate::json::Value::String(self.identifier.clone()),
            )]
            .into(),
        )
    }

    /// The warning as the run report's check row: the selection, named by
    /// its condition, examined as one unit and violated there, with the
    /// field it reads as the failure's details (LOOKUP-10).
    pub fn check(&self) -> crate::run_report::Check {
        crate::run_report::Check::warning(self.path.clone(), self.diagnostic())
    }
}
