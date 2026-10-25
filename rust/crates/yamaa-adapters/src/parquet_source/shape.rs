//! Check fixed-list row sizes from levels without allocating nested Arrow arrays.
use super::footer::{Kind, Repetition, Schema as Stored};
use super::reader::Error;
use arrow_schema::{DataType, Schema};

#[derive(Clone, Debug)]
pub(super) struct List {
    repetition: i16,
    /// The definition level of the list's repeated field: an element is
    /// present from this level on.
    definition: i16,
    expected: usize,
    count: Option<usize>,
}

impl List {
    pub fn observe(&mut self, definition: i16, repetition: i16) -> Result<(), Error> {
        if repetition < self.repetition {
            self.finish()?;
            // The list is present from one level below its repeated field. A
            // lower level is a null list, or a null struct or list element
            // above it, which has no size to check: pyarrow reads it as null.
            self.count = (definition >= self.definition - 1).then_some(0);
        }
        if definition >= self.definition && repetition <= self.repetition {
            let count = self.count.as_mut().ok_or(Error::Malformed)?;
            *count = count
                .checked_add(1)
                .filter(|n| *n <= self.expected)
                .ok_or(Error::Malformed)?;
        }
        Ok(())
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        if self.count.take().is_some_and(|n| n != self.expected) {
            return Err(Error::Malformed);
        }
        Ok(())
    }
}

pub(super) fn constraints(arrow: &Schema, stored: &Stored) -> Result<Vec<Vec<List>>, Error> {
    fn physical(
        stored: &Stored,
        node: usize,
        definition: i16,
        repeated: &[i16],
        columns: &mut Vec<Vec<i16>>,
    ) {
        let mut repeated = repeated.to_vec();
        let mut definition = definition;
        match stored.nodes[node].repetition {
            Some(Repetition::Optional) => definition += 1,
            Some(Repetition::Repeated) => {
                definition += 1;
                repeated.push(definition);
            }
            Some(Repetition::Required) | None => {}
        }
        match &stored.nodes[node].kind {
            Kind::Primitive(_) => columns.push(repeated),
            Kind::Group(children) => {
                for &child in children {
                    physical(stored, child, definition, &repeated, columns);
                }
            }
        }
    }
    fn logical(
        kind: &DataType,
        level: usize,
        inherited: &[(usize, i32)],
        columns: &mut Vec<Vec<(usize, i32)>>,
    ) {
        match kind {
            DataType::FixedSizeList(child, size) => {
                let mut inherited = inherited.to_vec();
                inherited.push((level, *size));
                logical(child.data_type(), level + 1, &inherited, columns);
            }
            DataType::List(child)
            | DataType::LargeList(child)
            | DataType::ListView(child)
            | DataType::LargeListView(child)
            | DataType::Map(child, _) => logical(child.data_type(), level + 1, inherited, columns),
            DataType::Struct(fields) => {
                for child in fields {
                    logical(child.data_type(), level, inherited, columns);
                }
            }
            DataType::Dictionary(_, value) => logical(value, level, inherited, columns),
            _ => columns.push(inherited.to_vec()),
        }
    }
    let mut repeated = Vec::new();
    physical(stored, 0, 0, &[], &mut repeated);
    let mut fixed = Vec::new();
    for field in arrow.fields() {
        logical(field.data_type(), 0, &[], &mut fixed);
    }
    if repeated.len() != fixed.len() {
        return Err(Error::Malformed);
    }
    fixed
        .into_iter()
        .zip(repeated)
        .map(|(fixed, repeated)| {
            fixed
                .into_iter()
                .map(|(level, size)| {
                    Ok(List {
                        repetition: i16::try_from(level + 1).map_err(|_| Error::Malformed)?,
                        definition: *repeated.get(level).ok_or(Error::Malformed)?,
                        expected: usize::try_from(size).map_err(|_| Error::Malformed)?,
                        count: None,
                    })
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    /// An optional list of three optional values: a null list is defined at
    /// level 0, an empty one at 1, a null value at 2 and a value at 3.
    fn flat() -> List {
        List {
            repetition: 1,
            definition: 2,
            expected: 3,
            count: None,
        }
    }
    #[test]
    fn checks_whole_rows_and_null_elements_without_materializing_lists() {
        let mut list = flat();
        for (definition, repetition) in [(3, 0), (2, 1), (3, 1), (3, 0), (3, 1), (3, 1)] {
            list.observe(definition, repetition).unwrap();
        }
        list.finish().unwrap();
        let mut short = flat();
        short.observe(3, 0).unwrap();
        short.observe(3, 1).unwrap();
        assert_eq!(short.finish(), Err(Error::Malformed));
    }
    /// A null list has no size, wherever it falls (#3366). An empty list is
    /// present and too short, and a value cannot continue a null list.
    #[test]
    fn null_rows_have_no_size_and_empty_rows_are_too_short() {
        let mut list = flat();
        for (definition, repetition) in [(0, 0), (3, 0), (3, 1), (3, 1), (0, 0)] {
            list.observe(definition, repetition).unwrap();
        }
        list.finish().unwrap();
        let mut empty = flat();
        empty.observe(1, 0).unwrap();
        assert_eq!(empty.finish(), Err(Error::Malformed));
        let mut continued = flat();
        continued.observe(0, 0).unwrap();
        assert_eq!(continued.observe(3, 1), Err(Error::Malformed));
    }
    /// The inner list of an optional list of optional lists: a level below
    /// 3 holds no inner list, and level 3 an empty one.
    #[test]
    fn absent_outer_elements_do_not_invent_inner_lists() {
        let nested = || List {
            repetition: 2,
            definition: 4,
            ..flat()
        };
        let mut list = nested();
        list.observe(0, 0).unwrap();
        list.observe(1, 0).unwrap();
        for (definition, repetition) in [(5, 0), (5, 2), (4, 2), (2, 1), (5, 1), (5, 2), (5, 2)] {
            list.observe(definition, repetition).unwrap();
        }
        list.finish().unwrap();
        let mut empty = nested();
        empty.observe(5, 0).unwrap();
        empty.observe(5, 2).unwrap();
        empty.observe(5, 2).unwrap();
        empty.observe(3, 1).unwrap();
        assert_eq!(empty.finish(), Err(Error::Malformed));
    }
}
