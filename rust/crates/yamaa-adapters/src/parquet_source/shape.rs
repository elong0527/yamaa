//! Check fixed-list row sizes from levels without allocating nested Arrow arrays.
use super::reader::Error;
use arrow_schema::{DataType, Schema};
use parquet::{
    basic::Repetition,
    schema::types::{SchemaDescriptor, Type},
};

#[derive(Clone, Debug)]
pub(super) struct List {
    repetition: i16,
    definition: i16,
    parent_definition: i16,
    expected: usize,
    count: Option<usize>,
}

impl List {
    pub fn observe(&mut self, definition: i16, repetition: i16) -> Result<(), Error> {
        if repetition < self.repetition {
            self.finish()?;
            self.count = (definition >= self.parent_definition).then_some(0);
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

pub(super) fn constraints(
    arrow: &Schema,
    stored: &SchemaDescriptor,
) -> Result<Vec<Vec<List>>, Error> {
    fn physical(node: &Type, definition: i16, repeated: &[i16], columns: &mut Vec<Vec<i16>>) {
        let mut repeated = repeated.to_vec();
        let mut definition = definition;
        if node.get_basic_info().has_repetition() {
            match node.get_basic_info().repetition() {
                Repetition::OPTIONAL => definition += 1,
                Repetition::REPEATED => {
                    definition += 1;
                    repeated.push(definition);
                }
                Repetition::REQUIRED => {}
            }
        }
        if node.is_primitive() {
            columns.push(repeated);
        } else {
            for child in node.get_fields() {
                physical(child, definition, &repeated, columns);
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
    physical(stored.root_schema(), 0, &[], &mut repeated);
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
                        repetition: i16::try_from(level + 1).map_err(|_| Error::Limit)?,
                        definition: *repeated.get(level).ok_or(Error::Malformed)?,
                        parent_definition: if level == 0 {
                            0
                        } else {
                            *repeated.get(level - 1).ok_or(Error::Malformed)?
                        },
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
    fn flat() -> List {
        List {
            repetition: 1,
            definition: 2,
            parent_definition: 0,
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
        let mut null = flat();
        null.observe(0, 0).unwrap();
        assert_eq!(null.finish(), Err(Error::Malformed));
    }
    #[test]
    fn absent_outer_elements_do_not_invent_inner_lists() {
        let mut list = List {
            repetition: 2,
            definition: 4,
            parent_definition: 2,
            ..flat()
        };
        list.observe(0, 0).unwrap();
        list.observe(1, 0).unwrap();
        for (definition, repetition) in [(5, 0), (5, 2), (4, 2), (5, 1), (5, 2), (5, 2)] {
            list.observe(definition, repetition).unwrap();
        }
        list.finish().unwrap();
    }
}
