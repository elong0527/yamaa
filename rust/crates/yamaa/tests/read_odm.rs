//! ODM XML clinical items of a local file through the public
//! [`yamaa::Engine::read_odm`], with no resolver.

use std::path::{Path, PathBuf};

#[cfg(feature = "odm")]
use yamaa::{DataType, StringArray, ODM_COLUMNS};
use yamaa::{Engine, EngineOptions, Error, OdmOptions, Table};

/// The items of the local file `path`, read as the command line reads them.
fn read_odm(path: &Path, options: &OdmOptions) -> Result<Table, Error> {
    Engine::new(EngineOptions::neutral("test".into(), String::new()))
        .read_odm(path.to_str().unwrap(), options)
}

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

/// A file of its own scratch directory, removed with it.
struct File {
    path: PathBuf,
    _directory: crate::support::Scratch,
}

impl File {
    fn new(name: &str, contents: &str) -> Self {
        let directory = crate::support::Scratch::new("read-odm");
        Self {
            path: directory.write(name, contents),
            _directory: directory,
        }
    }
}

#[cfg(feature = "odm")]
#[test]
fn clinical_items_read_into_the_fixed_table() {
    let file = File::new("items.xml", DOCUMENT);
    let table = read_odm(&file.path, &OdmOptions::new()).unwrap();
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
}

/// The one issue of a refusal, which a rejected report carries.
#[cfg(feature = "odm")]
#[track_caller]
fn refusal(result: Result<yamaa::Table, Error>) -> yamaa::Diagnostic {
    let Err(Error::Rejected(report)) = result else {
        panic!("the read must be refused with its issue");
    };
    let [issue] = report.diagnostics.as_slice() else {
        panic!("one issue, not {:?}", report.diagnostics);
    };
    issue.clone()
}

/// Every refusal carries its registered issue (#3030 D73, D158): the source
/// as given, its condition and ODM-1, and D39's error text names it.
#[cfg(feature = "odm")]
#[test]
fn refusals_carry_their_registered_issue() {
    use yamaa::{Action, ContextValue, Failure};
    let file = File::new(
        "refused.xml",
        &DOCUMENT.replace("IsNull=\"Yes\"", "IsNull=\"maybe\""),
    );
    let shown = file.path.display().to_string();
    let text = |value: &str| ContextValue::Text(value.to_owned());
    let invalid = refusal(read_odm(&file.path, &OdmOptions::new()));
    assert_eq!(
        (
            invalid.phase.as_str(),
            invalid.code.as_str(),
            invalid.rule.as_deref()
        ),
        ("ingest", "source_odm_attribute_invalid", Some("ODM-1"))
    );
    assert!(invalid.spec_path.is_empty());
    assert_eq!(invalid.context.get("path"), Some(&text(&shown)));
    assert_eq!(invalid.context.get("element"), Some(&text("ItemData")));
    assert_eq!(invalid.context.get("attribute"), Some(&text("IsNull")));
    assert_eq!(invalid.context.get("value"), Some(&text("maybe")));
    assert_eq!(
        invalid.message,
        format!("ItemData in {shown} has the unsupported IsNull value 'maybe'")
    );

    // An option below its minimum is refused at its name, before the source
    // is read.
    let options = OdmOptions::new().with_max_xml_depth(7);
    let error = read_odm(&file.path, &options).unwrap_err();
    let failure = Failure::from_error(Action::Read("edc/odm.xml"), &error);
    assert_eq!(
        failure.message,
        "cannot read edc/odm.xml: max_xml_depth has a value that is not permitted \
         [value_not_permitted, ODM-1]"
    );
    let option = refusal(Err(error));
    assert_eq!(
        (option.phase.as_str(), option.spec_path.as_slice()),
        ("validation", &["max_xml_depth".to_owned()][..])
    );
    assert_eq!(option.context.get("minimum"), Some(&ContextValue::Int(8)));

    let options = OdmOptions::new().with_archive_member("odm.xml");
    let member = refusal(read_odm(&file.path, &options));
    assert_eq!(member.code, "source_odm_archive_member");
    assert_eq!(
        member.message,
        format!("no member of {shown} can be read: archive_member requires a TAR input")
    );
}

/// Only a snapshot reads: a transactional document, and one without
/// `ClinicalData` such as a define.xml, are refused (ODM-1).
#[cfg(feature = "odm")]
#[test]
fn only_a_snapshot_with_clinical_data_reads() {
    let transactional = File::new(
        "transactional.xml",
        &DOCUMENT.replace(
            "ODMVersion=\"2.0\"",
            "ODMVersion=\"2.0\" FileType=\"Transactional\"",
        ),
    );
    let issue = refusal(read_odm(&transactional.path, &OdmOptions::new()));
    assert_eq!(issue.code, "source_odm_transactional");
    assert_eq!(
        issue.message,
        format!(
            "{} is not a snapshot: ODM declares FileType Transactional",
            transactional.path.display()
        )
    );
    let start = DOCUMENT.find("  <ClinicalData").unwrap();
    let end = DOCUMENT.find("</ODM>").unwrap();
    let metadata = File::new(
        "metadata.xml",
        &format!("{}{}", &DOCUMENT[..start], &DOCUMENT[end..]),
    );
    let issue = refusal(read_odm(&metadata.path, &OdmOptions::new()));
    assert_eq!(
        (issue.phase.as_str(), issue.code.as_str()),
        ("ingest", "source_odm_no_clinical_data")
    );
    // A source path that reaches no file is a caller's path (FILE-7).
    let scratch = crate::support::Scratch::new("read-odm-absent");
    let absent = scratch.join("absent.xml");
    let issue = refusal(read_odm(&absent, &OdmOptions::new()));
    assert_eq!(
        (issue.code.as_str(), issue.rule.as_deref()),
        ("resource_path_missing", Some("FILE-7"))
    );
    assert_eq!(
        issue.message,
        format!("{} does not exist", absent.display())
    );
}

#[cfg(not(feature = "odm"))]
#[test]
fn builds_without_the_odm_feature_report_the_capability() {
    let file = File::new("unavailable.xml", DOCUMENT);
    assert!(matches!(
        read_odm(&file.path, &OdmOptions::new()),
        Err(Error::Unavailable { feature: "odm" })
    ));
}
