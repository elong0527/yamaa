//! ODM XML clinical items through the public [`yamaa::read_odm`] facade.

use std::path::PathBuf;

use yamaa::{read_odm, DataType, Error, OdmOptions, StringArray, ODM_COLUMNS};

const DOCUMENT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ODM xmlns="http://www.cdisc.org/ns/odm/v2.0" ODMVersion="2.0">
  <Study OID="S"><MetaDataVersion OID="M">
    <ItemDef OID="AGE" Name="Age"/>
  </MetaDataVersion></Study>
  <ClinicalData StudyOID="S" MetaDataVersionOID="M">
    <SubjectData SubjectKey="01"><StudyEventData StudyEventOID="E">
      <ItemGroupData ItemGroupOID="F"><ItemGroupData ItemGroupOID="G">
        <ItemData ItemOID="AGE"><Value>42</Value></ItemData>
        <ItemData ItemOID="SEX" IsNull="Yes"/>
      </ItemGroupData></ItemGroupData>
    </StudyEventData></SubjectData>
  </ClinicalData>
</ODM>
"#;

struct File(PathBuf);

impl File {
    fn new(name: &str, contents: &str) -> Self {
        let path = std::env::temp_dir().join(format!("yamaa-facade-{}-{name}", std::process::id()));
        std::fs::write(&path, contents).unwrap();
        Self(path)
    }
}

impl Drop for File {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(feature = "odm")]
#[test]
fn clinical_items_read_into_the_fixed_table() {
    let file = File::new("items.xml", DOCUMENT);
    let table = read_odm(&file.0, &OdmOptions::new()).unwrap();
    assert_eq!(table.rows(), 2);
    let names = table
        .columns()
        .iter()
        .map(|field| field.name().as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ODM_COLUMNS);
    assert_eq!(table.columns()[29].data_type(), &DataType::Int64);
    let batch = &table.batches()[0];
    let text = |column: &str, row: usize| {
        let values = batch
            .column(table.schema().index_of(column).unwrap())
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        (!arrow_array::Array::is_null(values, row)).then(|| values.value(row).to_owned())
    };
    assert_eq!(text("ItemName", 0).as_deref(), Some("Age"));
    assert_eq!(text("Value", 0).as_deref(), Some("42"));
    assert_eq!(text("FormOID", 0).as_deref(), Some("F"));
    assert_eq!(text("IsNull", 1).as_deref(), Some("true"));
    assert_eq!(text("Value", 1), None);
    assert!(!table.ipc().unwrap().is_empty());
}

#[cfg(feature = "odm")]
#[test]
fn refusals_carry_a_stable_code_and_message() {
    let file = File::new(
        "refused.xml",
        &DOCUMENT.replace("IsNull=\"Yes\"", "IsNull=\"maybe\""),
    );
    let Err(Error::Odm(refusal)) = read_odm(&file.0, &OdmOptions::new()) else {
        panic!("the document must be refused");
    };
    assert_eq!(refusal.code(), "invalid_is_null");
    assert_eq!(refusal.message(), "unsupported IsNull value \"maybe\"");
    let options = OdmOptions::new().with_max_xml_depth(7);
    let Err(error) = read_odm(&file.0, &options) else {
        panic!("the options must be refused");
    };
    assert_eq!(
        error.to_string(),
        "ODM read failure (invalid_option): max_xml_depth must be at least 8"
    );
    let options = OdmOptions::new().with_archive_member("odm.xml");
    let Err(Error::Odm(refusal)) = read_odm(&file.0, &options) else {
        panic!("a plain XML input must refuse archive_member");
    };
    assert_eq!(refusal.code(), "archive_member");
}

#[cfg(not(feature = "odm"))]
#[test]
fn builds_without_the_odm_feature_report_the_capability() {
    let file = File::new("unavailable.xml", DOCUMENT);
    assert!(matches!(
        read_odm(&file.0, &OdmOptions::new()),
        Err(Error::Unavailable { feature: "odm" })
    ));
}
