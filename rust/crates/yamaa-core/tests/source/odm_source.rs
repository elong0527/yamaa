use yamaa_core::{
    odm_source::{self, SchemaError, StoredField},
    value::ColumnType,
};

#[test]
fn fixed_coordinates_ignore_untyped_vendor_fields_and_fold_only_ascii() {
    let names = [
        "Vendor",
        "value",
        "ITEMoid",
        "itemgrouprepeatkey",
        "itemgroupoid",
        "formrepeatkey",
        "formoid",
        "studyeventrepeatkey",
        "studyeventoid",
        "subjectkey",
        "metadataversionoid",
        "studyoid",
        "İtemOID",
    ];
    let fields = names
        .iter()
        .enumerate()
        .map(|(index, &name)| StoredField {
            name,
            kind: if index == 0 || index == 12 {
                None
            } else {
                Some(ColumnType::Str)
            },
        })
        .collect::<Vec<_>>();
    let binding = odm_source::bind_fields(&fields).unwrap().unwrap();
    assert_eq!(binding.stored(), &[11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1]);
    assert_eq!(odm_source::canonical("İtemOID"), None);
    assert_eq!(odm_source::canonical("ItemOID.extra"), None);
}

#[test]
fn metadata_findings_keep_missing_schema_order_and_stored_ambiguity_type_order() {
    let fields = [
        StoredField {
            name: "VALUE",
            kind: Some(ColumnType::Int),
        },
        StoredField {
            name: "FormOID",
            kind: None,
        },
        StoredField {
            name: "subjectkey",
            kind: Some(ColumnType::Str),
        },
        StoredField {
            name: "SubjectKey",
            kind: Some(ColumnType::Float),
        },
        StoredField {
            name: "vendor",
            kind: Some(ColumnType::Date),
        },
        StoredField {
            name: "STUDYOID",
            kind: Some(ColumnType::Str),
        },
        StoredField {
            name: "StudyOID",
            kind: Some(ColumnType::Str),
        },
    ];
    assert_eq!(
        odm_source::bind_fields(&fields).unwrap(),
        Err(vec![
            SchemaError::Missing {
                fields: vec![
                    "MetaDataVersionOID",
                    "StudyEventOID",
                    "StudyEventRepeatKey",
                    "FormRepeatKey",
                    "ItemGroupOID",
                    "ItemGroupRepeatKey",
                    "ItemOID",
                ]
            },
            SchemaError::Ambiguous {
                field: "SubjectKey",
                stored: vec!["subjectkey".into(), "SubjectKey".into()]
            },
            SchemaError::Ambiguous {
                field: "StudyOID",
                stored: vec!["STUDYOID".into(), "StudyOID".into()]
            },
            SchemaError::Type {
                field: "Value",
                stored_field: "VALUE".into(),
                kind: Some(ColumnType::Int)
            },
            SchemaError::Type {
                field: "FormOID",
                stored_field: "FormOID".into(),
                kind: None
            },
        ])
    );
}

#[test]
fn whole_metadata_admission_precedes_owned_findings() {
    let large = "v".repeat(1_048_577);
    let fields = [StoredField {
        name: &large,
        kind: None,
    }];
    assert_eq!(
        odm_source::bind_fields(&fields),
        Err(odm_source::AdmissionError::MetadataLimit)
    );
}

struct Records {
    schema: yamaa_core::table::TableSchema,
    values: Vec<Vec<yamaa_core::value::Value>>,
    reads: std::cell::RefCell<Vec<(usize, usize)>>,
    failure: Option<(usize, usize)>,
    marker: std::rc::Rc<String>,
}
impl Records {
    fn new() -> Self {
        use yamaa_core::{
            table::{Column, TableSchema},
            value::Value,
        };
        Self {
            schema: TableSchema::new(
                odm_source::FIELDS
                    .iter()
                    .map(|&name| Column {
                        name: name.into(),
                        kind: ColumnType::Str,
                    })
                    .collect(),
            )
            .unwrap(),
            values: vec![vec![Value::Str("id".into()); 11]; 2],
            reads: Default::default(),
            failure: None,
            marker: std::rc::Rc::new("opaque original".into()),
        }
    }
}
impl yamaa_core::table::TableAccess for Records {
    type Error = std::rc::Rc<String>;
    fn schema(&self) -> &yamaa_core::table::TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.values.len()
    }
    fn cell(
        &self,
        row: usize,
        column: usize,
    ) -> Result<yamaa_core::table::ValueRef<'_>, yamaa_core::table::CellError<Self::Error>> {
        self.reads.borrow_mut().push((row, column));
        if self.failure == Some((row, column)) {
            return Err(yamaa_core::table::CellError::Access(self.marker.clone()));
        }
        let value = self
            .values
            .get(row)
            .and_then(|values| values.get(column))
            .ok_or(yamaa_core::table::CellError::OutOfBounds { row, column })?;
        Ok(yamaa_core::table::ValueRef::from(value))
    }
}

#[test]
fn required_identifiers_report_all_fields_in_fixed_order_and_storage_record_coordinates() {
    use yamaa_core::value::Value;
    let mut records = Records::new();
    records.values[1][0] = Value::Missing;
    for row in &mut records.values {
        row[2] = Value::Missing;
        row[9] = Value::Missing;
        row[4] = Value::Missing;
        row[10] = Value::Missing;
    }
    assert_eq!(
        odm_source::missing_values(&records, 2).unwrap(),
        [
            odm_source::MissingValues {
                field: "StudyOID",
                record: 3,
                records: 1
            },
            odm_source::MissingValues {
                field: "SubjectKey",
                record: 2,
                records: 2
            },
            odm_source::MissingValues {
                field: "ItemOID",
                record: 2,
                records: 2
            },
        ]
    );
    assert_eq!(
        *records.reads.borrow(),
        [
            (0, 0),
            (1, 0),
            (0, 1),
            (1, 1),
            (0, 2),
            (1, 2),
            (0, 3),
            (1, 3),
            (0, 5),
            (1, 5),
            (0, 7),
            (1, 7),
            (0, 9),
            (1, 9)
        ]
    );
    assert_eq!(
        odm_source::missing_values(&records, 1).unwrap()[0].record,
        2
    );
}

#[test]
fn a_later_original_ingestion_read_failure_suppresses_partial_findings_without_retry() {
    let mut records = Records::new();
    records.values[0][0] = yamaa_core::value::Value::Missing;
    records.failure = Some((1, 1));
    let odm_source::RecordError::Cell {
        row: 1,
        column: 1,
        error: yamaa_core::table::CellError::Access(marker),
    } = odm_source::missing_values(&records, 2).unwrap_err()
    else {
        panic!("original cell failure required")
    };
    assert!(std::rc::Rc::ptr_eq(&marker, &records.marker));
    assert_eq!(*records.reads.borrow(), [(0, 0), (1, 0), (0, 1), (1, 1)]);
}
