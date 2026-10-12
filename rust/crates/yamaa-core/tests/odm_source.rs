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
