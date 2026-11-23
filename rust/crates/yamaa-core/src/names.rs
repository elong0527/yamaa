//! The first position of each name in a list, looked up instead of searched
//! for.
//!
//! A specification declares as many columns as its bytes hold, with no fixed
//! maximum (#4773). A check that asks, for each name of a list, whether an
//! earlier entry repeats it, or which entry of another list has that name,
//! looks the name up here: searching the list once for each name is work
//! that grows with the square of the list (#4782).
use std::collections::BTreeMap;

/// Each distinct name of a list with the position of its first entry: what a
/// search of the list from its start answers.
pub(crate) struct Positions<'a>(BTreeMap<&'a str, usize>);

impl<'a> Positions<'a> {
    /// The names of a list, in its order.
    pub(crate) fn of(names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut positions = BTreeMap::new();
        for (position, name) in names.into_iter().enumerate() {
            positions.entry(name).or_insert(position);
        }
        Self(positions)
    }

    /// The position of the first entry named `name`.
    pub(crate) fn first(&self, name: &str) -> Option<usize> {
        self.0.get(name).copied()
    }

    /// Whether an entry is named `name`.
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    /// Whether an entry before `position` is named `name`: whether the entry
    /// at `position`, of that name, repeats an earlier one.
    pub(crate) fn repeats(&self, position: usize, name: &str) -> bool {
        self.first(name).is_some_and(|first| first < position)
    }
}

#[cfg(test)]
mod tests {
    use super::Positions;

    /// Seam: the lookup every list check shares, against the search it
    /// replaces, entry by entry.
    #[test]
    fn a_lookup_answers_as_a_search_of_the_list_from_its_start() {
        let list = ["B", "A", "", "B", "a", "A", "B"];
        let positions = Positions::of(list);
        for (position, name) in list.into_iter().enumerate() {
            let searched = list[..position].contains(&name);
            assert_eq!(positions.repeats(position, name), searched, "{position}");
        }
        for (name, first) in [
            ("B", Some(0)),
            ("A", Some(1)),
            ("", Some(2)),
            ("a", Some(4)),
            ("C", None),
        ] {
            assert_eq!(positions.first(name), first, "{name:?}");
            assert_eq!(positions.contains(name), first.is_some(), "{name:?}");
        }
        // No entry stands before the first.
        assert!(!positions.repeats(0, "B"));
        assert!(!Positions::of([]).contains(""));
    }
}
