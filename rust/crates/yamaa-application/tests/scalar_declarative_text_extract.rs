#![cfg(feature = "yaml")]
//! Text extract: the capacity of `str_extract`'s pattern findings (TEXT-6).
//! The test declares sixty malformed patterns of 601 characters, built by
//! code, and reads `PreparedRun::prepare`'s findings, a seam no case
//! operation reaches, before the file route reports each one in full. The
//! family's other cases are data in `tests/fixtures/diagnostics/text_extract.json`
//! (#3798).
#[test]
fn all_invalid_patterns_keep_complete_findings_before_zero_row_execution() {
    let mut columns = vec![
        yamaa_adapters::json!({"name":"ID","type":"str","derivation":"SRC.ID"}),
        yamaa_adapters::json!({"name":"TEXT","type":"str","derivation":"SRC.TEXT"}),
    ];
    let mut names = vec!["ID".to_owned(), "TEXT".to_owned()];
    // Each declared pattern remains a separate finding within fixed width limits.
    for index in 0..60 {
        let name = format!("V{index}");
        names.push(name.clone());
        columns.push(yamaa_adapters::json!({"name":name,"type":"str","derivation":{"str_extract":{"source":"TEXT","pattern":format!("[{}","a".repeat(600)),"group":0}}}));
    }
    let spec = yamaa_adapters::json!({"yamaa":"1.0","type":"spec","domain":"TEST","keys":["ID"],"input":{"SRC":"input.csv"},"output":{"path":"output.csv","columns":names},"columns":columns});
    // Prepared in the language it is written in.
    let document = crate::support::specification_run::prepare(
        yamaa_adapters::json_wire::encode(&spec).as_bytes(),
    );
    let Err(yamaa_application::specification_run::Error::Prepare(
        yamaa_core::specification::PrepareError::Invalid(findings),
    )) = yamaa_application::specification_run::PreparedRun::prepare(document)
    else {
        panic!("all malformed patterns must retain their language findings")
    };
    assert_eq!(findings.len(), 60);
    let expected = (0..60).map(|index| yamaa_adapters::json!({
        "phase":"validation", "condition":"invalid_regex", "rule":"TEXT-6",
        "spec_paths":[format!("columns.V{index}.derivation.str_extract.pattern")],
        "context":{"pattern":format!("[{}", "a".repeat(600)), "reason":"a character class is never closed"}
    })).collect::<Vec<_>>();
    let study = crate::support::study::Study::new("str-extract");
    study.write("spec.yaml", yamaa_adapters::json_wire::encode(&spec));
    study.write("input.csv", b"ID,TEXT\n");
    for missing in [false, true] {
        if missing {
            std::fs::remove_file(study.path("input.csv")).unwrap();
        }
        let Err(issues) = study.run("spec.yaml") else {
            panic!("invalid declarations must precede study access")
        };
        assert_eq!(
            issues
                .iter()
                .map(crate::support::study::issue_row)
                .collect::<Vec<_>>(),
            expected
        );
        assert!(!std::path::Path::new(&study.path("output.csv")).exists());
    }
}
