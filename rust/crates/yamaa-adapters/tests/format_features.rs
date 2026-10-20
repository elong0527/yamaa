//! Capability controls also run when no document or storage reader is selected.
use yamaa_adapters::{
    arrow_table::ArrowTable, parquet_artifact, parquet_source, sas_source, yaml_decode,
};
use yamaa_core::{
    specification::{SourceDeclaration, SourceProfile},
    table::{TableAccess, ValueRef},
};

fn source(profile: SourceProfile) -> SourceDeclaration {
    SourceDeclaration {
        name: "SRC".into(),
        path: "held.parquet".into(),
        types: vec![],
        profile,
        empty_string_present: false,
        ordinal: None,
        encoding: Some("windows-1252".into()),
    }
}

fn csv_table() -> ArrowTable {
    let table = yamaa_adapters::typed_csv::parse(b"ID,TEXT\n1,alpha\n2,beta\n", &[]).unwrap();
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Str("beta"));
    table
}

#[test]
fn csv_and_arrow_remain_available_without_physical_readers() {
    let table = csv_table();
    assert_eq!(
        yamaa_adapters::csv_artifact::render(&table, &[0, 1], None).unwrap(),
        b"ID,TEXT\n1,alpha\n2,beta\n"
    );
    // The public Arrow table and its IPC stream need no physical reader.
    let public = yamaa_adapters::public_table::PublicTable::from_snapshot(&table).unwrap();
    let bytes = public.ipc().unwrap();
    let restored = arrow_ipc::reader::StreamReader::try_new(bytes.as_slice(), None)
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    let text = restored.column(1).as_any();
    let text = text.downcast_ref::<arrow_array::StringArray>().unwrap();
    assert_eq!(text.value(0), "alpha");
}

#[test]
fn yaml_capability_is_explicit_at_document_admission() {
    #[cfg(feature = "yaml")]
    {
        let document = yaml_decode::decode_yaml(b"key: [value, 9007199254740993]\n").unwrap();
        assert!(!document.locations.is_empty());
        assert!(matches!(
            yaml_decode::decode_yaml(b"x: 1\nx: 2\n"),
            Err(yaml_decode::DecodeFailure::InvalidYaml { .. })
        ));
        yamaa_adapters::shipped_schema::capture().unwrap();
    }
    #[cfg(not(feature = "yaml"))]
    {
        assert!(matches!(
            yaml_decode::decode_yaml(b"key: value\n"),
            Err(yaml_decode::DecodeFailure::Unavailable)
        ));
        let error = yamaa_adapters::shipped_schema::capture().unwrap_err();
        assert!(matches!(
            error,
            yamaa_adapters::specification_source::Error::Decode {
                error: yaml_decode::DecodeFailure::Unavailable,
                ..
            }
        ));
        let yamaa_adapters::specification_diagnostics::Rejection::Fault(finding) =
            yamaa_adapters::specification_diagnostics::capture_failure(&error)
        else {
            panic!("an omitted YAML reader must remain an engine fault");
        };
        assert_eq!(
            finding.code,
            yamaa_core::diagnostic::ConditionCode::ApplicationRejected
        );
        assert_eq!(finding.context["stage"].as_str(), Some("capture"));
        assert_eq!(
            finding.context["code"].as_str(),
            Some("yaml_reader_unavailable")
        );
    }
}

#[test]
fn parquet_source_and_artifact_share_one_capability() {
    let table = csv_table();
    #[cfg(feature = "parquet")]
    {
        let bytes = parquet_artifact::render(&table, &[0, 1]).unwrap();
        let restored = parquet_source::parse(&bytes).unwrap();
        assert_eq!(restored.row_count(), 2);
        assert_eq!(restored.cell(1, 1).unwrap(), ValueRef::Str("beta"));
        assert!(matches!(
            parquet_source::parse(b"malformed"),
            Err(parquet_source::Error::Malformed)
        ));
    }
    #[cfg(not(feature = "parquet"))]
    {
        assert!(matches!(
            parquet_artifact::render(&table, &[0, 1]),
            Err(parquet_artifact::Error::Unavailable)
        ));
        for result in [
            parquet_source::parse(b"malformed"),
            parquet_source::parse_odm(b"malformed"),
        ] {
            let error = result.unwrap_err();
            assert!(matches!(error, parquet_source::Error::ReaderUnavailable));
            assert!(error.finding("SRC", "held.parquet").is_none());
        }
        let error = yamaa_adapters::specification_run::Error::ParquetSource(
            parquet_source::Error::ReaderUnavailable,
        );
        let yamaa_adapters::specification_diagnostics::Rejection::Fault(finding) =
            yamaa_adapters::specification_diagnostics::failure(
                &error,
                Some(&source(SourceProfile::Parquet)),
            )
        else {
            panic!("an omitted Parquet reader must remain an engine fault");
        };
        assert_eq!(
            finding.code,
            yamaa_core::diagnostic::ConditionCode::ApplicationRejected
        );
        assert_eq!(finding.context["stage"].as_str(), Some("ingest"));
        assert_eq!(
            finding.context["code"].as_str(),
            Some("parquet_reader_unavailable")
        );
    }
}

#[test]
fn odm_reader_is_an_independent_capability() {
    let path = std::env::temp_dir().join(format!("yamaa-format-odm-{}.xml", std::process::id()));
    std::fs::write(&path, r#"<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3"/>"#).unwrap();
    let result = yamaa_adapters::odm_xml::read(&path, &yamaa_core::odm_items::Options::default());
    std::fs::remove_file(&path).unwrap();
    #[cfg(feature = "odm")]
    {
        let table = result.unwrap();
        assert_eq!(table.rows(), 0);
        assert_eq!(table.columns().len(), yamaa_core::odm_items::COLUMNS.len());
    }
    #[cfg(not(feature = "odm"))]
    assert!(matches!(
        result,
        Err(yamaa_adapters::odm_xml::Error::Unavailable)
    ));
}

#[test]
fn sas_reader_remains_independent_of_yaml_and_parquet() {
    let declaration = source(SourceProfile::Xport);
    let bytes = include_bytes!("fixtures/sas/fractional.xpt");
    let result = sas_source::parse(bytes, &declaration);
    #[cfg(feature = "readstat")]
    assert!(result.unwrap().row_count() > 0);
    #[cfg(not(feature = "readstat"))]
    assert!(matches!(result, Err(sas_source::Error::Unavailable)));
}
