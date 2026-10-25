//! Capability controls also run when no document or storage reader is selected.
#[allow(dead_code)]
#[path = "support/scratch.rs"]
mod scratch;

use yamaa_adapters::{
    column_table::ColumnTable, parquet_artifact, parquet_source, sas_source, yaml_decode,
};
use yamaa_core::{
    decoder::Decoder as _,
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
        byte_order_mark: false,
    }
}

/// An omitted reader is an engine fault naming its step and closed code.
#[cfg(not(all(feature = "yaml", feature = "parquet", feature = "odm")))]
fn assert_fault(
    rejection: yamaa_adapters::specification_diagnostics::Rejection,
    stage: &str,
    code: &str,
) {
    let yamaa_adapters::specification_diagnostics::Rejection::Fault(issue) = &rejection else {
        panic!("an omitted reader is an engine fault: {rejection:?}");
    };
    assert_eq!(issue.condition(), "engine_rejected", "{issue:?}");
    assert_eq!(issue.context["step"].as_str(), Some(stage), "{issue:?}");
    assert_eq!(issue.context["code"].as_str(), Some(code), "{issue:?}");
}

fn csv_table() -> ColumnTable {
    let table = yamaa_adapters::typed_csv::parse(b"ID,TEXT\n1,alpha\n2,beta\n", &[]).unwrap();
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Str("beta"));
    table
}

#[test]
fn csv_and_public_tables_remain_available_without_physical_readers() {
    let table = csv_table();
    assert_eq!(
        yamaa_adapters::csv_artifact::render(&table, &[0, 1], None).unwrap(),
        b"ID,TEXT\n1,alpha\n2,beta\n"
    );
    // The public table a host receives needs no physical reader.
    let public = yamaa_adapters::public_table::PublicTable::from_table(&table, &[0, 1]).unwrap();
    let text = [Some("alpha"), Some("beta")].into_iter().collect();
    assert_eq!(
        public.column(1),
        &yamaa_adapters::public_table::PublicColumn::Str(text)
    );
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
        yamaa_adapters::shipped_schema::capture_versioned(yamaa_core::language::FileType::Spec)
            .unwrap();
    }
    #[cfg(not(feature = "yaml"))]
    {
        assert!(matches!(
            yaml_decode::decode_yaml(b"key: value\n"),
            Err(yaml_decode::DecodeFailure::Unavailable)
        ));
        let error =
            yamaa_adapters::shipped_schema::capture_versioned(yamaa_core::language::FileType::Spec)
                .unwrap_err();
        assert!(matches!(
            error,
            yamaa_adapters::specification_source::Error::Decode {
                error: yaml_decode::DecodeFailure::Unavailable,
                ..
            }
        ));
        assert_fault(
            yamaa_adapters::specification_diagnostics::capture_failure(&error),
            "capture",
            "yaml_reader_unavailable",
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
        let error = yamaa_adapters::source_decoding::Error::ParquetSource(
            parquet_source::Error::ReaderUnavailable,
        );
        assert_fault(
            yamaa_adapters::specification_diagnostics::failure(
                &error,
                Some(&source(SourceProfile::Parquet)),
            ),
            "ingest",
            "parquet_reader_unavailable",
        );
    }
}

#[test]
fn odm_reader_is_an_independent_capability() {
    const DOCUMENT: &str = r#"<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3"><ClinicalData StudyOID="S" MetaDataVersionOID="M"/></ODM>"#;
    let directory = scratch::Scratch::new("format-odm");
    let path = directory.write("input.xml", DOCUMENT);
    let result = yamaa_adapters::odm_xml::read(&path, &yamaa_core::odm_items::Options::default());
    // A specification's `.xml` input reads its held bytes through the same
    // reader, in the shared source decode (INPUT-1, ODM-1).
    let mut declaration = source(SourceProfile::OdmXml);
    declaration.path = "held.xml".into();
    declaration.encoding = None;
    let decoded = yamaa_adapters::decoder::Decoder::<std::convert::Infallible>::default()
        .table(
            &declaration,
            DOCUMENT.as_bytes(),
            yamaa_core::specification::SourceInterpretation::Ordinary,
            None,
        )
        .map(|snapshot| snapshot.into_table().expect("an input read whole"));
    #[cfg(feature = "odm")]
    {
        let table = result.unwrap();
        assert_eq!(table.rows(), 0);
        assert_eq!(table.columns().len(), yamaa_core::odm_items::COLUMNS.len());
        let decoded = decoded.unwrap();
        assert_eq!(decoded.row_count(), 0);
        assert_eq!(
            decoded.schema().columns().len(),
            yamaa_core::odm_items::COLUMNS.len()
        );
    }
    #[cfg(not(feature = "odm"))]
    {
        assert!(matches!(
            result,
            Err(yamaa_adapters::odm_xml::Error::Unavailable)
        ));
        let error = decoded.unwrap_err();
        assert!(matches!(
            error,
            yamaa_adapters::source_decoding::Error::OdmSource(
                yamaa_adapters::odm_xml::Error::Unavailable
            )
        ));
        assert_fault(
            yamaa_adapters::specification_diagnostics::failure(&error, Some(&declaration)),
            "ingest",
            "odm_reader_unavailable",
        );
    }
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
