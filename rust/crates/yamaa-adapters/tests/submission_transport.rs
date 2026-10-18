#![cfg(feature = "yaml")]
//! Transport benchmarks through the shared Define service.
//!
//! Every benchmark whose `benchmark.yaml` declares `define` runs that call over
//! its independently authored published artifacts. A positive case must hold and
//! save exactly the bytes of `expected/`; a negative case must fail with the first
//! issue `expected/error.yaml` asserts and write nothing.
mod transport {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };
    use yamaa_adapters::file_define::{Define, PublicationState};
    use yamaa_core::{json::Value, project_function::Language};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "yamaa-transport-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(yamaa_adapters::canonical_path::canonicalize(path).unwrap())
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn benchmarks() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../benchmarks")
            .canonicalize()
            .unwrap()
    }

    fn yaml(path: &Path) -> Value {
        let text = fs::read(path).unwrap();
        let document = yamaa_adapters::yaml_decode::decode_yaml(&text)
            .unwrap()
            .document;
        fn value(document: &yamaa_core::schema::Document, id: usize) -> Value {
            use yamaa_core::schema::DocumentNode as N;
            match &document.nodes()[id] {
                N::Null => Value::Null,
                N::Boolean(v) => (*v).into(),
                N::Integer(v) => yamaa_adapters::json_wire::decode(v).unwrap(),
                N::Float(v) => yamaa_adapters::json!(v),
                N::Text(v) => v.clone().into(),
                N::Sequence(ids) => ids.iter().map(|&id| value(document, id)).collect(),
                N::Mapping(pairs) => pairs
                    .iter()
                    .map(|&(key, item)| {
                        (
                            value(document, key).as_str().unwrap().to_owned(),
                            value(document, item),
                        )
                    })
                    .collect(),
            }
        }
        value(&document, document.root())
    }

    /// Copy a case without its expected truth, then publish its expected tables
    /// beside the specification, as the producing domain build saved them.
    fn stage(case: &Path) -> Temp {
        fn copy(from: &Path, to: &Path) {
            for entry in fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name();
                if ["expected", "README.md", "benchmark.yaml"]
                    .iter()
                    .any(|skipped| name == *skipped)
                {
                    continue;
                }
                let target = to.join(&name);
                if entry.file_type().unwrap().is_dir() {
                    fs::create_dir_all(&target).unwrap();
                    copy(&entry.path(), &target);
                } else {
                    fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        let temp = Temp::new();
        copy(case, &temp.0);
        for entry in fs::read_dir(case.join("expected")).unwrap() {
            let path = entry.unwrap().path();
            if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("csv" | "parquet")
            ) {
                fs::copy(&path, temp.0.join(path.file_name().unwrap())).unwrap();
            }
        }
        temp
    }

    fn run(case: &Path, temp: &Temp) -> Define {
        let metadata = yaml(&case.join("benchmark.yaml"));
        let define = &metadata["define"];
        Define::prepare(
            temp.0
                .join(define["environment"].as_str().unwrap())
                .to_str()
                .unwrap(),
            define["standard"].as_str().unwrap(),
            Some(define["created"].as_str().unwrap()),
            "0.0.0",
            Language::Python,
        )
    }

    fn subset(want: &Value, got: &Value) -> bool {
        match (want, got) {
            (Value::Object(want), Value::Object(got)) => want
                .iter()
                .all(|(key, value)| got.get(key).is_some_and(|got| subset(value, got))),
            _ => want == got,
        }
    }

    fn define_cases() -> Vec<PathBuf> {
        let mut cases: Vec<_> = fs::read_dir(benchmarks())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|case| {
                case.join("benchmark.yaml").is_file()
                    && !yaml(&case.join("benchmark.yaml"))["define"].is_null()
            })
            .collect();
        cases.sort();
        cases
    }

    #[test]
    fn every_define_benchmark_matches_its_independent_truth_in_both_hosts() {
        let cases = define_cases();
        let names: Vec<_> = cases
            .iter()
            .map(|case| case.file_name().unwrap().to_str().unwrap().to_owned())
            .collect();
        for required in [
            "adam-adsl-xpt",
            "negative-transport-define-name",
            "negative-transport-display-decimals",
            "negative-transport-empty-string",
            "negative-transport-label-too-long",
            "negative-transport-name-too-long",
            "negative-transport-non-ascii",
            "negative-transport-trailing-blank",
            "sdtm-dm-metadata",
            "sdtm-dm-xpt",
            "sdtm-ta-sponsor-epoch",
        ] {
            assert!(names.iter().any(|name| name == required), "{required}");
        }
        for case in &cases {
            let name = case.file_name().unwrap().to_str().unwrap();
            // A case publishing Parquet needs the Parquet codec.
            if !cfg!(feature = "parquet")
                && fs::read_dir(case.join("expected")).unwrap().any(|entry| {
                    entry
                        .unwrap()
                        .path()
                        .extension()
                        .is_some_and(|extension| extension == "parquet")
                })
            {
                continue;
            }
            for host in [Language::Python, Language::R] {
                let temp = stage(case);
                let metadata = yaml(&case.join("benchmark.yaml"));
                let define = &metadata["define"];
                let mut result = Define::prepare(
                    temp.0
                        .join(define["environment"].as_str().unwrap())
                        .to_str()
                        .unwrap(),
                    define["standard"].as_str().unwrap(),
                    Some(define["created"].as_str().unwrap()),
                    "0.0.0",
                    host,
                );
                let oracle = case.join("expected/error.yaml");
                if oracle.is_file() {
                    let want = yaml(&oracle);
                    let issue = result.issues().first().unwrap_or_else(|| {
                        panic!("{name}: Define reported no issue");
                    });
                    let got = yamaa_adapters::json!({
                        "phase": issue.phase,
                        "condition": issue.condition,
                        "requirement": issue.requirement,
                        "spec_paths": issue.spec_paths,
                        "context": issue.context.clone(),
                    });
                    assert!(subset(&want, &got), "{name}: {got:?} vs {want:?}");
                    assert!(result.save().is_err(), "{name}");
                    for written in fs::read_dir(&temp.0).unwrap() {
                        let file = written.unwrap().file_name();
                        let file = file.to_str().unwrap();
                        assert!(
                            !file.ends_with(".xpt")
                                && !file.ends_with(".json")
                                && file != "define.xml",
                            "{name}: a refused Define wrote {file}"
                        );
                    }
                    continue;
                }
                assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
                let held: Vec<_> = result
                    .artifacts()
                    .map(|(written, bytes, state)| (written.to_owned(), bytes.to_vec(), state))
                    .collect();
                assert_eq!(held.last().unwrap().0, "define.xml", "{name}");
                for (written, bytes, state) in &held {
                    assert_eq!(*state, PublicationState::Ready);
                    let truth = fs::read(case.join("expected").join(written)).unwrap();
                    assert!(
                        *bytes == truth,
                        "{name}: held {written} differs from expected"
                    );
                }
                assert!(result.save().unwrap());
                for (written, bytes, _) in &held {
                    assert_eq!(&fs::read(temp.0.join(written)).unwrap(), bytes, "{name}");
                }
            }
        }
    }

    /// The written transport files read back through the repository's own
    /// SAS Transport reader profile as the published values.
    #[cfg(feature = "readstat")]
    #[test]
    fn written_transport_files_read_back_through_the_xport_reader() {
        use yamaa_core::{
            specification::{SourceDeclaration, SourceProfile},
            table::{TableAccess, ValueRef},
            value::ColumnType as T,
        };
        for (name, file, types) in [
            ("sdtm-dm-xpt", "dm.xpt", vec![("AGE", T::Int)]),
            (
                "adam-adsl-xpt",
                "adsl.xpt",
                vec![
                    ("AGE", T::Int),
                    ("TRTSDT", T::Date),
                    ("TRTSDTM", T::DateTime),
                ],
            ),
        ] {
            let case = benchmarks().join(name);
            let temp = stage(&case);
            let mut result = run(&case, &temp);
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            let bytes = fs::read(temp.0.join(file)).unwrap();
            let source = SourceDeclaration {
                name: "SRC".into(),
                path: file.into(),
                profile: SourceProfile::Xport,
                types: types
                    .iter()
                    .map(|(column, kind)| ((*column).into(), *kind))
                    .collect(),
                encoding: Some("ascii".into()),
                empty_string_present: false,
                ordinal: None,
            };
            let table = yamaa_adapters::sas_source::parse(&bytes, &source).unwrap();
            let expected =
                fs::read_to_string(case.join("expected").join(file.replace(".xpt", ".csv")))
                    .unwrap();
            let mut lines = expected.lines();
            let header: Vec<_> = lines.next().unwrap().split(',').collect();
            let names: Vec<_> = table
                .schema()
                .columns()
                .iter()
                .map(|column| column.name.as_str())
                .collect();
            assert_eq!(names, header, "{name}");
            let rows: Vec<Vec<&str>> = lines.map(|line| line.split(',').collect()).collect();
            assert_eq!(table.row_count(), rows.len(), "{name}");
            for (row, cells) in rows.iter().enumerate() {
                for (column, cell) in cells.iter().enumerate() {
                    let actual = match table.cell(row, column).unwrap() {
                        ValueRef::Missing => String::new(),
                        ValueRef::Str(text) => text.to_owned(),
                        ValueRef::Int(value) => value.to_string(),
                        ValueRef::Float(value) => yamaa_core::conversion::float_text(value),
                        ValueRef::Date(value) => value.to_string(),
                        ValueRef::DateTime(value) => value.to_string(),
                        ValueRef::Bool(_) => unreachable!("a transport file has no Boolean"),
                    };
                    assert_eq!(actual, *cell, "{name} row {row} {}", header[column]);
                }
            }
        }
    }

    /// XPT-9 and TIME-20: an analysis time is a numeric variable carrying its
    /// declared `TIME8.` in its NAMESTR, and reads back as its seconds.
    #[test]
    fn an_analysis_time_is_a_numeric_variable_with_its_time_format() {
        use yamaa_core::{
            specification::{SourceDeclaration, SourceProfile},
            table::{TableAccess, ValueRef},
            value::ColumnType as T,
        };
        let case = benchmarks().join("adam-adae-analysis-time");
        let temp = stage(&case);
        let environment = temp.0.join("environment.yaml");
        let text = fs::read_to_string(&environment)
            .unwrap()
            .replace("transport: dataset_json", "transport: xpt");
        fs::write(&environment, text).unwrap();
        let mut result = run(&case, &temp);
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        assert!(result.save().unwrap());
        let bytes = fs::read(temp.0.join("adae.xpt")).unwrap();
        // The seventh NAMESTR, after the 640 header bytes (XPT-2, XPT-4).
        let namestr = &bytes[640 + 140 * 6..640 + 140 * 7];
        assert_eq!(&namestr[0..2], &[0, 1], "numeric");
        assert_eq!(&namestr[4..6], &[0, 8], "eight bytes");
        assert_eq!(&namestr[8..16], b"ASTTM   ");
        assert_eq!(&namestr[56..64], b"TIME    ");
        assert_eq!(&namestr[64..66], &[0, 8], "format width");
        assert_eq!(&namestr[66..68], &[0, 0], "format decimals");
        let source = SourceDeclaration {
            name: "SRC".into(),
            path: "adae.xpt".into(),
            profile: SourceProfile::Xport,
            types: [("AESEQ", T::Int), ("ASTTM", T::Int)]
                .iter()
                .map(|(column, kind)| ((*column).into(), *kind))
                .collect(),
            encoding: Some("ascii".into()),
            empty_string_present: false,
            ordinal: None,
        };
        let table = yamaa_adapters::sas_source::parse(&bytes, &source).unwrap();
        let times: Vec<_> = (0..table.row_count())
            .map(|row| match table.cell(row, 6).unwrap() {
                ValueRef::Int(value) => Some(value),
                ValueRef::Missing => None,
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            times,
            [Some(30615), Some(50700), Some(0), None, Some(75600)]
        );
    }

    /// JSON-9 and JSON-10: under `adam` a date or datetime keeps its ISO value
    /// in Dataset-JSON and asks for the integer the document submits.
    #[test]
    fn adam_dataset_json_targets_the_integer_its_document_submits() {
        let case = benchmarks().join("adam-adsl-xpt");
        let temp = stage(&case);
        let project = temp.0.join("project.yaml");
        let text = fs::read_to_string(&project)
            .unwrap()
            .replace("datasets: xpt", "datasets: json");
        fs::write(&project, text).unwrap();
        let result = run(&case, &temp);
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        let artifacts: Vec<_> = result.artifacts().collect();
        assert_eq!(artifacts[0].0, "adsl.json");
        let json = std::str::from_utf8(artifacts[0].1).unwrap();
        for line in [
            r#"{"itemOID": "IT.ADSL.AGE", "name": "AGE", "label": "Age", "dataType": "integer", "length": 3, "displayFormat": "3."}"#,
            r#"{"itemOID": "IT.ADSL.TRTSDT", "name": "TRTSDT", "label": "Date of First Exposure to Treatment", "dataType": "date", "targetDataType": "integer", "length": 8, "displayFormat": "DATE9."}"#,
            r#"{"itemOID": "IT.ADSL.TRTSDTM", "name": "TRTSDTM", "label": "Datetime of First Exposure to Treatment", "dataType": "datetime", "targetDataType": "integer", "length": 8, "displayFormat": "DATETIME20."}"#,
            r#"["CATH", "CATH-UCSD-0001", "VITAMIN D3", 34, 172.5, "2025-01-08", "2025-01-08T08:30:00"]"#,
        ] {
            assert!(json.contains(line), "{line}\n{json}");
        }
        // The document is unchanged by the transport it names.
        let xml = std::str::from_utf8(artifacts[1].1).unwrap();
        let expected = fs::read_to_string(case.join("expected/define.xml"))
            .unwrap()
            .replace("adsl.xpt", "adsl.json");
        assert_eq!(xml, expected);
    }

    /// SUB-24: one column name has one length across the section's datasets,
    /// and a SUPP-- dataset is measured on its own.
    #[test]
    fn shared_column_lengths_span_datasets_and_supplemental_datasets_stand_alone() {
        let case = benchmarks().join("sdtm-dm-xpt");
        let temp = stage(&case);
        let spec = fs::read_to_string(temp.0.join("spec.yaml")).unwrap();
        // A second dataset with a longer STUDYID, and a SUPP-- dataset whose
        // USUBJID is shorter than DM's.
        fs::write(
            temp.0.join("ae.yaml"),
            spec.replace("domain: DM", "domain: AE")
                .replace("path: dm.csv", "path: ae.csv"),
        )
        .unwrap();
        fs::write(
            temp.0.join("suppdm.yaml"),
            spec.replace("domain: DM", "domain: SUPPDM")
                .replace("path: dm.csv", "path: suppdm.csv"),
        )
        .unwrap();
        let dm = fs::read_to_string(temp.0.join("dm.csv")).unwrap();
        fs::write(
            temp.0.join("ae.csv"),
            dm.replace("\nDM,CATH,", "\nDM,CATHETER,"),
        )
        .unwrap();
        let header = dm.lines().next().unwrap();
        let first = dm.lines().nth(1).unwrap().replace("CATH-UCSD-0001", "C-1");
        fs::write(temp.0.join("suppdm.csv"), format!("{header}\n{first}\n")).unwrap();
        let project = temp.0.join("project.yaml");
        let text = fs::read_to_string(&project).unwrap().replace(
            "  - spec.yaml",
            "  - spec.yaml\n  - ae.yaml\n  - suppdm.yaml",
        );
        fs::write(&project, text).unwrap();
        let result = run(&case, &temp);
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        let artifacts: Vec<_> = result.artifacts().map(|(name, _, _)| name).collect();
        assert_eq!(artifacts, ["dm.xpt", "ae.xpt", "suppdm.xpt", "define.xml"]);
        let xml = std::str::from_utf8(result.artifacts().nth(3).unwrap().1).unwrap();
        for (item, length) in [
            ("IT.DM.STUDYID", "8"),
            ("IT.AE.STUDYID", "8"),
            ("IT.SUPPDM.STUDYID", "4"),
            ("IT.DM.USUBJID", "16"),
            ("IT.SUPPDM.USUBJID", "3"),
        ] {
            let begin = xml.find(&format!("<ItemDef OID=\"{item}\"")).unwrap();
            let element = &xml[begin..begin + xml[begin..].find('>').unwrap()];
            assert!(
                element.contains(&format!("Length=\"{length}\"")),
                "{element}"
            );
        }
    }

    /// JSON-3: a dataset declared to have no data has no transport file and no
    /// leaf, and a dataset without rows that does not declare it is refused.
    #[test]
    fn datasets_without_data_have_no_file_and_empty_ones_must_declare_it() {
        let case = benchmarks().join("sdtm-dm-xpt");
        let temp = stage(&case);
        let spec = fs::read_to_string(temp.0.join("spec.yaml")).unwrap();
        fs::write(
            temp.0.join("ae.yaml"),
            spec.replace("domain: DM", "domain: AE")
                .replace("path: dm.csv", "path: ae.csv")
                .replace(
                    "  repeating: false\n",
                    "  repeating: false\n  has_no_data: true\n  comment: No adverse events were collected.\n",
                ),
        )
        .unwrap();
        let project = temp.0.join("project.yaml");
        let text = fs::read_to_string(&project)
            .unwrap()
            .replace("  - spec.yaml", "  - spec.yaml\n  - ae.yaml");
        fs::write(&project, text).unwrap();
        let mut result = run(&case, &temp);
        assert!(result.issues().is_empty(), "{:?}", result.issues());
        let names: Vec<_> = result.artifacts().map(|(name, _, _)| name).collect();
        assert_eq!(names, ["dm.xpt", "define.xml"]);
        let xml = std::str::from_utf8(result.artifacts().nth(1).unwrap().1).unwrap();
        let begin = xml.find("<ItemGroupDef OID=\"IG.AE\"").unwrap();
        let group = &xml[begin..begin + xml[begin..].find("</ItemGroupDef>").unwrap()];
        assert!(group.contains("def:HasNoData=\"Yes\""), "{group}");
        assert!(!group.contains("def:ArchiveLocationID"), "{group}");
        assert!(!group.contains("<def:leaf"), "{group}");
        assert!(result.save().unwrap());
        assert!(!temp.0.join("ae.xpt").exists());
        // The same dataset without the declaration and without rows fails.
        let temp = stage(&case);
        let header = fs::read_to_string(temp.0.join("dm.csv")).unwrap();
        let header = header.lines().next().unwrap();
        fs::write(temp.0.join("dm.csv"), format!("{header}\n")).unwrap();
        let mut result = run(&case, &temp);
        let issue = &result.issues()[0];
        assert_eq!(
            (issue.phase.as_str(), issue.condition.as_str()),
            ("output", "transport_dataset_empty")
        );
        assert_eq!(issue.spec_paths, ["datasets.DM.artifact"]);
        assert!(result.save().is_err());
        assert!(!temp.0.join("dm.xpt").exists());
    }

    /// JSON-2: a transport file never replaces a declared input.
    #[test]
    fn a_transport_file_never_replaces_a_declared_input() {
        let case = benchmarks().join("sdtm-dm-xpt");
        let temp = stage(&case);
        fs::rename(temp.0.join("input/dm_raw.csv"), temp.0.join("dm.xpt")).unwrap();
        let spec = fs::read_to_string(temp.0.join("spec.yaml"))
            .unwrap()
            .replace(
                "DM_RAW: input/dm_raw.csv",
                "DM_RAW: {path: dm.xpt, encoding: ascii}",
            );
        fs::write(temp.0.join("spec.yaml"), spec).unwrap();
        let mut result = run(&case, &temp);
        let issue = &result.issues()[0];
        assert_eq!(issue.condition, "transport_path_collision");
        assert_eq!(issue.spec_paths, ["sdtm.specs[0].input.DM_RAW.path"]);
        assert!(result.save().is_err());
    }
}
