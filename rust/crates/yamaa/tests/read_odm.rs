//! ODM XML clinical items of a local file through the public
//! [`yamaa::Engine::read_odm`], with no resolver.

use std::path::{Path, PathBuf};

#[cfg(feature = "odm")]
use yamaa::{DataType, StringArray, ODM_COLUMNS};
use yamaa::{Engine, EngineOptions, Failure, FailureKind, OdmOptions, Table};

/// The items of the local file `path`, read as the command line reads them.
fn read_odm(path: &Path, options: &OdmOptions) -> Result<Table, Failure> {
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

/// The one issue of a refusal, which a rejected failure carries.
#[cfg(feature = "odm")]
#[track_caller]
fn refusal(result: Result<yamaa::Table, Failure>) -> yamaa::Diagnostic {
    let failure = result.expect_err("the read must be refused with its issue");
    assert!(matches!(failure.kind, FailureKind::Rejected), "{failure:?}");
    let [issue] = failure.diagnostics.as_slice() else {
        panic!("one issue, not {:?}", failure.diagnostics);
    };
    issue.clone()
}

/// Every refusal carries its registered issue (#3030 D73, D158): the source
/// as given, its condition and ODM-1, and D39's error text names it.
#[cfg(feature = "odm")]
#[test]
fn refusals_carry_their_registered_issue() {
    use yamaa::ContextValue;
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
    assert!(invalid.spec_paths.is_empty());
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
    let failure = read_odm(&file.path, &options).unwrap_err();
    assert_eq!(
        failure.message,
        format!(
            "cannot read {shown}: max_xml_depth has the value 7, which is not permitted \
             [value_not_permitted, ODM-1]"
        )
    );
    let option = refusal(Err(failure));
    assert_eq!(
        (option.phase.as_str(), option.spec_paths.as_slice()),
        ("validation", &["max_xml_depth".to_owned()][..])
    );
    assert_eq!(option.context.get("minimum"), Some(&ContextValue::Int(8)));
    assert_eq!(option.context.get("value"), Some(&ContextValue::Int(7)));

    let options = OdmOptions::new().with_archive_member("odm.xml");
    let member = refusal(read_odm(&file.path, &options));
    assert_eq!(member.code.as_str(), "source_odm_archive_member");
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
    assert_eq!(issue.code.as_str(), "source_odm_transactional");
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
        read_odm(&file.path, &OdmOptions::new()).map_err(|failure| failure.kind),
        Err(FailureKind::Unavailable { feature: "odm" })
    ));
}

/// The hosts' ODM limit arguments (#5080) reach the host-internal
/// `odm_options` seam, which no reader of a file reaches: a whole number
/// within the limit is admitted, which only R gives as a double, and then
/// only exactly, within 2^53; Python's limits are `int`s (`bindings.yaml`,
/// `types.count`). Any other value refuses at its argument, before a source
/// is read, as `value_not_permitted` (ODM-1), with the registry's message
/// `{argument} has the value {value}, which is not permitted` and the
/// context `{minimum, value}`. R's whole double shows as the integer it
/// spells; Python's float keeps its own spelling.
#[test]
fn a_host_odm_limit_is_a_whole_number_that_only_r_gives_as_a_double() {
    use yamaa::host_internal::odm_options;
    use yamaa::{ContextValue as V, HostLanguage};
    let depth = |language, value: V| {
        odm_options(
            language,
            "unread.xml",
            V::Int(10_000),
            V::Int(268_435_456),
            value,
        )
        .map(|options| options.max_xml_depth)
        .map_err(|failure| {
            let issue = &failure.diagnostics[0];
            Box::new((
                failure.diagnostics.len(),
                issue.phase.as_str(),
                issue.code.as_str().to_owned(),
                issue.rule.clone(),
                issue.spec_paths.clone(),
                issue.message.clone(),
                issue.context.to_json().to_owned(),
            ))
        })
    };
    // The whole refusal of `max_xml_depth`, whose minimum is 8, showing
    // `shown`.
    let refused = |shown: &str| {
        Err(Box::new((
            1,
            "validation",
            "value_not_permitted".to_owned(),
            Some("ODM-1".to_owned()),
            vec!["max_xml_depth".to_owned()],
            format!("max_xml_depth has the value {shown}, which is not permitted"),
            format!("{{\"minimum\":8,\"value\":{shown}}}"),
        )))
    };
    // Each case: its label, the value, and what Python and R give.
    let cases = [
        ("an integer", V::Int(64), Ok(64), Ok(64)),
        ("a whole double", V::Float(64.0), refused("64.0"), Ok(64)),
        (
            "a fraction",
            V::Float(64.5),
            refused("64.5"),
            refused("64.5"),
        ),
        (
            "an integer below the minimum",
            V::Int(7),
            refused("7"),
            refused("7"),
        ),
        (
            "a double below the minimum",
            V::Float(7.0),
            refused("7.0"),
            refused("7"),
        ),
        (
            "a double beyond 2^53",
            V::Float(2f64.powi(53) + 2.0),
            refused("9007199254740994.0"),
            refused("9007199254740994.0"),
        ),
    ];
    for (label, value, python, r) in cases {
        assert_eq!(
            (
                depth(HostLanguage::Python, value.clone()),
                depth(HostLanguage::R, value)
            ),
            (python, r),
            "{label}"
        );
    }
}
