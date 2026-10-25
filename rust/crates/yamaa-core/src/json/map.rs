//! The JSON object map (#3870): its entries in one vector sorted by key, so
//! a small object, such as one offending row's keys, takes the room its
//! entries do rather than a tree node's. Lookups search the sorted keys;
//! iteration is in key order, as a `BTreeMap`'s.
use std::borrow::Borrow;
use std::fmt;

/// A map whose entries are held sorted by key, one per key.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Map<K, V> {
    entries: Vec<(K, V)>,
}

impl<K, V> Default for Map<K, V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<K: fmt::Debug, V: fmt::Debug> fmt::Debug for Map<K, V> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_map()
            .entries(self.entries.iter().map(|(key, value)| (key, value)))
            .finish()
    }
}

impl<K, V> Map<K, V> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter(self.entries.iter())
    }
    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        IterMut(self.entries.iter_mut())
    }
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &K> + ExactSizeIterator + Clone {
        self.entries.iter().map(|(key, _)| key)
    }
    pub fn values(&self) -> impl DoubleEndedIterator<Item = &V> + ExactSizeIterator + Clone {
        self.entries.iter().map(|(_, value)| value)
    }
    pub fn values_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut V> + ExactSizeIterator {
        self.entries.iter_mut().map(|(_, value)| value)
    }
    pub fn into_keys(self) -> impl DoubleEndedIterator<Item = K> + ExactSizeIterator {
        self.entries.into_iter().map(|(key, _)| key)
    }
    pub fn into_values(self) -> impl DoubleEndedIterator<Item = V> + ExactSizeIterator {
        self.entries.into_iter().map(|(_, value)| value)
    }
    pub fn first_key_value(&self) -> Option<(&K, &V)> {
        self.entries.first().map(|(key, value)| (key, value))
    }
    pub fn last_key_value(&self) -> Option<(&K, &V)> {
        self.entries.last().map(|(key, value)| (key, value))
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&K, &mut V) -> bool) {
        self.entries.retain_mut(|(key, value)| keep(key, value));
    }
}

impl<K: Ord, V> Map<K, V> {
    fn position<Q: Ord + ?Sized>(&self, key: &Q) -> Result<usize, usize>
    where
        K: Borrow<Q>,
    {
        self.entries
            .binary_search_by(|(held, _)| held.borrow().cmp(key))
    }
    pub fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.position(key).ok().map(|at| &self.entries[at].1)
    }
    pub fn get_key_value<Q: Ord + ?Sized>(&self, key: &Q) -> Option<(&K, &V)>
    where
        K: Borrow<Q>,
    {
        self.position(key).ok().map(|at| {
            let (key, value) = &self.entries[at];
            (key, value)
        })
    }
    pub fn get_mut<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
    {
        self.position(key).ok().map(|at| &mut self.entries[at].1)
    }
    pub fn contains_key<Q: Ord + ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.position(key).is_ok()
    }
    /// Insert `value` at `key`, returning the value it replaces; the held
    /// key stays, as a `BTreeMap` keeps it.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        // Keys arriving in order append.
        if self.entries.last().is_none_or(|(last, _)| *last < key) {
            self.entries.push((key, value));
            return None;
        }
        match self.position(&key) {
            Ok(at) => Some(std::mem::replace(&mut self.entries[at].1, value)),
            Err(at) => {
                self.entries.insert(at, (key, value));
                None
            }
        }
    }
    pub fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        self.remove_entry(key).map(|(_, value)| value)
    }
    pub fn remove_entry<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<(K, V)>
    where
        K: Borrow<Q>,
    {
        self.position(key).ok().map(|at| self.entries.remove(at))
    }
    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        match self.position(&key) {
            Ok(at) => Entry::Occupied(OccupiedEntry { map: self, at }),
            Err(at) => Entry::Vacant(VacantEntry { map: self, at, key }),
        }
    }
    pub fn append(&mut self, other: &mut Self) {
        for (key, value) in std::mem::take(&mut other.entries) {
            self.insert(key, value);
        }
    }
}

/// One key's place in a [`Map`].
pub enum Entry<'a, K, V> {
    Occupied(OccupiedEntry<'a, K, V>),
    Vacant(VacantEntry<'a, K, V>),
}

pub struct OccupiedEntry<'a, K, V> {
    map: &'a mut Map<K, V>,
    at: usize,
}

pub struct VacantEntry<'a, K, V> {
    map: &'a mut Map<K, V>,
    at: usize,
    key: K,
}

impl<'a, K, V> OccupiedEntry<'a, K, V> {
    pub fn key(&self) -> &K {
        &self.map.entries[self.at].0
    }
    pub fn get(&self) -> &V {
        &self.map.entries[self.at].1
    }
    pub fn get_mut(&mut self) -> &mut V {
        &mut self.map.entries[self.at].1
    }
    pub fn into_mut(self) -> &'a mut V {
        &mut self.map.entries[self.at].1
    }
    pub fn insert(&mut self, value: V) -> V {
        std::mem::replace(self.get_mut(), value)
    }
    pub fn remove(self) -> V {
        self.map.entries.remove(self.at).1
    }
}

impl<'a, K, V> VacantEntry<'a, K, V> {
    pub fn key(&self) -> &K {
        &self.key
    }
    pub fn insert(self, value: V) -> &'a mut V {
        self.map.entries.insert(self.at, (self.key, value));
        &mut self.map.entries[self.at].1
    }
}

impl<'a, K, V> Entry<'a, K, V> {
    pub fn key(&self) -> &K {
        match self {
            Self::Occupied(entry) => entry.key(),
            Self::Vacant(entry) => entry.key(),
        }
    }
    pub fn or_insert(self, value: V) -> &'a mut V {
        self.or_insert_with(|| value)
    }
    pub fn or_insert_with(self, value: impl FnOnce() -> V) -> &'a mut V {
        match self {
            Self::Occupied(entry) => entry.into_mut(),
            Self::Vacant(entry) => entry.insert(value()),
        }
    }
    pub fn or_default(self) -> &'a mut V
    where
        V: Default,
    {
        self.or_insert_with(V::default)
    }
    pub fn and_modify(mut self, change: impl FnOnce(&mut V)) -> Self {
        if let Self::Occupied(entry) = &mut self {
            change(entry.get_mut());
        }
        self
    }
}

/// The entries of a [`Map`], by reference, in key order.
#[derive(Clone)]
pub struct Iter<'a, K, V>(std::slice::Iter<'a, (K, V)>);

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(key, value)| (key, value))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}
impl<K, V> DoubleEndedIterator for Iter<'_, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|(key, value)| (key, value))
    }
}
impl<K, V> ExactSizeIterator for Iter<'_, K, V> {}

/// The entries of a [`Map`], their values mutable, in key order.
pub struct IterMut<'a, K, V>(std::slice::IterMut<'a, (K, V)>);

impl<'a, K, V> Iterator for IterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(key, value)| (&*key, value))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}
impl<K, V> DoubleEndedIterator for IterMut<'_, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back().map(|(key, value)| (&*key, value))
    }
}
impl<K, V> ExactSizeIterator for IterMut<'_, K, V> {}

/// The entries of a [`Map`], owned, in key order.
pub struct IntoIter<K, V>(std::vec::IntoIter<(K, V)>);

impl<K, V> Iterator for IntoIter<K, V> {
    type Item = (K, V);
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}
impl<K, V> DoubleEndedIterator for IntoIter<K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }
}
impl<K, V> ExactSizeIterator for IntoIter<K, V> {}

impl<K, V> IntoIterator for Map<K, V> {
    type Item = (K, V);
    type IntoIter = IntoIter<K, V>;
    fn into_iter(self) -> Self::IntoIter {
        IntoIter(self.entries.into_iter())
    }
}

impl<'a, K, V> IntoIterator for &'a Map<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, K, V> IntoIterator for &'a mut Map<K, V> {
    type Item = (&'a K, &'a mut V);
    type IntoIter = IterMut<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for Map<K, V> {
    /// The last value for a repeated key wins, as in a `BTreeMap`.
    fn from_iter<I: IntoIterator<Item = (K, V)>>(entries: I) -> Self {
        let mut map = Self::new();
        map.extend(entries);
        map
    }
}

impl<K: Ord, V> Extend<(K, V)> for Map<K, V> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, entries: I) {
        for (key, value) in entries {
            self.insert(key, value);
        }
    }
}

impl<K: Ord, V, const N: usize> From<[(K, V); N]> for Map<K, V> {
    fn from(entries: [(K, V); N]) -> Self {
        entries.into_iter().collect()
    }
}

impl<K: Ord + Borrow<Q>, Q: Ord + ?Sized, V> std::ops::Index<&Q> for Map<K, V> {
    type Output = V;
    fn index(&self, key: &Q) -> &V {
        self.get(key).expect("no entry found for key")
    }
}

#[cfg(test)]
mod tests {
    use super::Map;
    use std::collections::BTreeMap;

    /// The map answers every operation as a `BTreeMap` does, the container
    /// it replaces: its entries in key order whatever the insertion order, a
    /// replaced value returned and its key kept, the last of repeated keys
    /// collected, and entries found, changed and removed by key.
    #[test]
    fn the_map_answers_as_a_btree_map_does() {
        let keys = ["m", "b", "z", "b", "a", "m", "q"];
        let (mut map, mut tree) = (Map::new(), BTreeMap::new());
        for (value, &key) in keys.iter().enumerate() {
            assert_eq!(
                map.insert(key.to_owned(), value),
                tree.insert(key.to_owned(), value),
                "insert {key}"
            );
        }
        let entries =
            |map: &Map<String, usize>| map.iter().map(|(k, v)| (k.clone(), *v)).collect::<Vec<_>>();
        let reference = |tree: &BTreeMap<String, usize>| {
            tree.iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect::<Vec<_>>()
        };
        assert_eq!(entries(&map), reference(&tree), "the entries in key order");
        *map.entry("c".into()).or_insert(0) += 7;
        *tree.entry("c".into()).or_insert(0) += 7;
        *map.entry("z".into()).or_default() += 1;
        *tree.entry("z".into()).or_default() += 1;
        assert_eq!(map.remove("m"), tree.remove("m"), "remove m");
        assert_eq!(map.remove("y"), tree.remove("y"), "remove an absent key");
        map.retain(|key, _| key != "q");
        tree.retain(|key, _| key != "q");
        assert_eq!(
            entries(&map),
            reference(&tree),
            "after entry, remove and retain"
        );
        assert_eq!(
            (map.get("b"), map.contains_key("a"), map.len()),
            (tree.get("b"), tree.contains_key("a"), tree.len()),
            "lookups"
        );
        let collected: Map<&str, usize> = [("k", 1), ("j", 2), ("k", 3)].into_iter().collect();
        let reference: BTreeMap<&str, usize> = [("k", 1), ("j", 2), ("k", 3)].into_iter().collect();
        assert_eq!(
            collected.into_iter().collect::<Vec<_>>(),
            reference.into_iter().collect::<Vec<_>>(),
            "the last of repeated keys collected"
        );
    }
}
