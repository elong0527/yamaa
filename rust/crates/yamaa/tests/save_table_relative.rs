//! A relative OUT is a caller path: it is made absolute against the working
//! directory, as `read_odm`'s source is, and the save returns the absolute
//! path it wrote (#3030 D38, D134), as the command line's
//! `yamaa read-odm SOURCE OUT` usually passes it. This binary holds this one
//! test, since it changes the process's working directory.
#![cfg(feature = "odm")]

use yamaa::{Engine, EngineOptions, OdmOptions};

const ODM: &str = r#"<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3">
  <ClinicalData StudyOID="S" MetaDataVersionOID="M">
    <SubjectData SubjectKey="01"><StudyEventData StudyEventOID="E">
      <FormData FormOID="F"><ItemGroupData ItemGroupOID="G">
        <ItemData ItemOID="AGE" Value="42"/>
      </ItemGroupData></FormData>
    </StudyEventData></SubjectData>
  </ClinicalData>
</ODM>
"#;

#[test]
fn a_relative_path_saves_in_the_working_directory_and_returns_it_absolute() {
    let root =
        std::env::temp_dir().join(format!("yamaa-save-table-relative-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let root = yamaa_adapters::canonical_path::canonicalize(root).unwrap();
    std::fs::write(root.join("odm.xml"), ODM).unwrap();
    let before = std::env::current_dir().unwrap();
    std::env::set_current_dir(&root).unwrap();
    let engine = Engine::new(EngineOptions::neutral("save-table-test".into(), "0".into()));
    let saved = engine
        .read_odm("odm.xml", &OdmOptions::new())
        .map(|table| engine.save_table(&table, "items.csv"));
    std::env::set_current_dir(before).unwrap();
    let written = std::fs::read(root.join("items.csv"));
    let expected = root.join("items.csv").to_str().unwrap().to_owned();
    #[cfg(windows)]
    let expected = expected.replace('\\', "/");
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(saved.unwrap().unwrap(), [expected]);
    assert!(written.unwrap().starts_with(b"ODMVersion,FileOID,"));
}
