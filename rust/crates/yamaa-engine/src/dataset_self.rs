//! Borrow completed donor rows in construction order, without snapshots or study reads.
use super::*;

pub(super) struct Donors<'a, E> {
    rows: &'a [Candidate],
    schema: &'a TableSchema,
    error: core::marker::PhantomData<E>,
}
impl<'a, E> Donors<'a, E> {
    pub(super) fn new(rows: &'a [Candidate], schema: &'a TableSchema) -> Self {
        Self {
            rows,
            schema,
            error: core::marker::PhantomData,
        }
    }
}
impl<E> TableAccess for Donors<'_, E> {
    type Error = E;
    fn schema(&self) -> &TableSchema {
        self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<E>> {
        self.rows
            .get(row)
            .and_then(|candidate| candidate.values.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn donor_text_lends_the_completed_value_allocation_and_checks_coordinates() {
        let schema = TableSchema::new(vec![yamaa_core::table::Column {
            name: "V".into(),
            kind: yamaa_core::value::ColumnType::Str,
        }])
        .unwrap();
        let candidates = vec![Candidate {
            selection_identity: SelectionIdentity::Candidate(0),
            members: vec![],
            values: vec![Value::Str("borrowed donor Ω".into())],
            completed: vec![true],
        }];
        let donors = Donors::<Infallible>::new(&candidates, &schema);
        let Value::Str(original) = &candidates[0].values[0] else {
            unreachable!()
        };
        let ValueRef::Str(reading) = donors.cell(0, 0).unwrap() else {
            unreachable!()
        };
        assert_eq!(reading.as_ptr(), original.as_ptr());
        assert_eq!(reading.len(), original.len());
        assert_eq!(donors.row_count(), 1);
        assert_eq!(
            donors.cell(1, 0),
            Err(CellError::OutOfBounds { row: 1, column: 0 })
        );
        assert_eq!(
            donors.cell(0, 1),
            Err(CellError::OutOfBounds { row: 0, column: 1 })
        );
    }
}
