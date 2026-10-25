#![cfg(feature = "yaml")]
//! Transport benchmarks through the shared Define service.
//!
//! Every benchmark whose `benchmark.yaml` declares `define` runs that call over
//! its independently authored published artifacts. A positive case must hold and
//! save exactly the bytes of `expected/`; a negative case must fail with the first
//! issue `expected/error.yaml` asserts and write nothing. The tests that staged a
//! benchmark, composed it once and compared its issues, its document or its
//! Dataset-JSON are define cases of `tests/fixtures/diagnostics/shared_define.json`
//! (#3798); the tests left walk the benchmarks or read saved transport bytes.
mod transport {
    use crate::support::Edit as _;
    use std::{
        fs,
        path::{Path, PathBuf},
    };
    use yamaa_adapters::file_define::{Define, PublicationState};
    use yamaa_core::{json::Value, project_function::Language, project_scope};

    struct Temp(crate::support::Scratch);
    impl Temp {
        fn new() -> Self {
            let path = crate::support::Scratch::canonical("transport");
            Self(path)
        }
    }

    fn benchmarks() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../benchmarks")
            .canonicalize()
            .unwrap()
    }

    /// Write the benchmark's project and specification as 1.0 writes them
    /// over a staged copy, for a test that edits the project's text: the
    /// benchmark moves to 1.0 in its own pull request, and the copies keep
    /// the edit in one language (#3422).
    fn convert(name: &str, temp: &Temp) {
        let converted = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/converted")
            .join(name);
        for file in ["project.yaml", "spec.yaml"] {
            fs::copy(converted.join(file), temp.0.join(file)).unwrap();
        }
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
            project_scope::section(define["standard"].as_str().unwrap()).unwrap(),
            Some(define["created"].as_str().unwrap()),
            "0.0.0",
            Some(Language::Python),
            None,
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
        crate::support::benchmarks::all()
            .into_iter()
            .filter(|case| !yaml(&case.join("benchmark.yaml"))["define"].is_null())
            .map(|case| case.canonicalize().unwrap())
            .collect()
    }

    /// Already data: every define benchmark's own `expected/` files and
    /// `error.yaml`, for both hosts.
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
            "schema-transport-dataset-ndjson",
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
                    project_scope::section(define["standard"].as_str().unwrap()).unwrap(),
                    Some(define["created"].as_str().unwrap()),
                    "0.0.0",
                    Some(host),
                    None,
                );
                let oracle = case.join("expected/error.yaml");
                if oracle.is_file() {
                    let want = yaml(&oracle);
                    let issue = result.issues().first().unwrap_or_else(|| {
                        panic!("{name}: Define reported no issue");
                    });
                    let got = crate::support::study::issue_row(issue);
                    assert!(subset(&want, &got), "{name}: {got:?} vs {want:?}");
                    assert!(result.save().is_err(), "{name}");
                    for written in fs::read_dir(&temp.0).unwrap() {
                        let file = written.unwrap().file_name();
                        let file = file.to_str().unwrap();
                        assert!(
                            !file.ends_with(".xpt")
                                && !file.ends_with(".json")
                                && !file.ends_with(".ndjson")
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

    /// Seam: the saved SAS Transport bytes read back through
    /// `sas_source::parse`, which no reader of the replay calls. The written
    /// transport files read back through the repository's own SAS Transport
    /// reader profile as the published values.
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
                byte_order_mark: false,
                empty_string_present: false,
                ordinal: None,
            };
            let table = yamaa_adapters::sas_source::parse(&bytes, &source).unwrap();
            let expected =
                fs::read_to_string(case.join("expected").join(file.edit(".xpt", ".csv"))).unwrap();
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

    /// Seam: byte offsets of a NAMESTR in saved SAS Transport bytes, and their
    /// values read back through `sas_source::parse`. XPT-9 and TIME-20: an
    /// analysis time is a numeric variable carrying its declared `TIME8.` in
    /// its NAMESTR, and reads back as its seconds.
    #[test]
    fn an_analysis_time_is_a_numeric_variable_with_its_time_format() {
        use yamaa_core::{
            specification::{SourceDeclaration, SourceProfile},
            table::{TableAccess, ValueRef},
            value::ColumnType as T,
        };
        let case = benchmarks().join("adam-adae-analysis-time");
        let temp = stage(&case);
        convert("adam-adae-analysis-time", &temp);
        let project = temp.0.join("project.yaml");
        let text = fs::read_to_string(&project)
            .unwrap()
            .edit("transport: dataset_json", "transport: xpt");
        fs::write(&project, text).unwrap();
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
            byte_order_mark: false,
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

    /// Already data: each define benchmark's single-object Dataset-JSON
    /// golden. JSON-4 and JSON-13: the newline-delimited form of a dataset is the
    /// single-object form's object without `rows` on its first line, then one
    /// row per line, so the two representations carry one value. The byte
    /// goldens fix each form; this proves the seam between them, which no
    /// golden reader reaches.
    #[test]
    fn the_newline_delimited_form_carries_the_single_object_forms_value() {
        let mut checked = 0;
        for case in define_cases() {
            if case.join("expected/error.yaml").is_file() {
                continue;
            }
            let name = case.file_name().unwrap().to_str().unwrap().to_owned();
            let metadata = yaml(&case.join("benchmark.yaml"));
            let environment = metadata["define"]["environment"]
                .as_str()
                .unwrap()
                .to_owned();
            let temp = stage(&case);
            let project = temp.0.join(&environment);
            let text = fs::read_to_string(&project).unwrap();
            if !text.contains("transport: dataset_json") {
                continue;
            }
            fs::write(
                &project,
                text.edit("transport: dataset_json", "transport: dataset_ndjson"),
            )
            .unwrap();
            let result = run(&case, &temp);
            assert!(result.issues().is_empty(), "{name}: {:?}", result.issues());
            for (written, bytes, _) in result.artifacts() {
                let Some(dataset) = written.strip_suffix(".ndjson") else {
                    continue;
                };
                let text = std::str::from_utf8(bytes).unwrap();
                assert!(
                    text.ends_with('\n') && !text.contains('\r'),
                    "{name}: {written} line terminators"
                );
                let mut lines = text.strip_suffix('\n').unwrap().split('\n');
                let mut value: serde_json::Value =
                    serde_json::from_str(lines.next().unwrap()).unwrap();
                assert!(
                    value.get("rows").is_none(),
                    "{name}: {written} carries rows on its first line"
                );
                let rows: Vec<serde_json::Value> = lines
                    .map(|line| serde_json::from_str(line).unwrap())
                    .collect();
                assert_eq!(
                    value["records"],
                    serde_json::Value::from(rows.len()),
                    "{name}: {written} records"
                );
                value["rows"] = serde_json::Value::Array(rows);
                let golden: serde_json::Value = serde_json::from_slice(
                    &fs::read(case.join("expected").join(format!("{dataset}.json"))).unwrap(),
                )
                .unwrap();
                assert_eq!(
                    value, golden,
                    "{name}: {written} against its single-object golden"
                );
                checked += 1;
            }
        }
        assert!(checked >= 2, "{checked} datasets compared");
    }

    /// Seam: the composition writes SAS Transport files for datasets no
    /// benchmark publishes, which a case's `saved` states only by their whole
    /// bytes, and no authored file holds them. SUB-24: one column name has one
    /// length across the section's datasets, and a SUPP-- dataset is measured
    /// on its own.
    #[test]
    fn shared_column_lengths_span_datasets_and_supplemental_datasets_stand_alone() {
        let case = benchmarks().join("sdtm-dm-xpt");
        let temp = stage(&case);
        let spec = fs::read_to_string(temp.0.join("spec.yaml")).unwrap();
        // A second dataset with a longer STUDYID, and a SUPP-- dataset whose
        // USUBJID is shorter than DM's.
        fs::write(
            temp.0.join("ae.yaml"),
            spec.edit("domain: DM", "domain: AE")
                .edit("path: dm.csv", "path: ae.csv"),
        )
        .unwrap();
        fs::write(
            temp.0.join("suppdm.yaml"),
            spec.edit("domain: DM", "domain: SUPPDM")
                .edit("path: dm.csv", "path: suppdm.csv"),
        )
        .unwrap();
        let dm = fs::read_to_string(temp.0.join("dm.csv")).unwrap();
        fs::write(
            temp.0.join("ae.csv"),
            dm.replace("\nDM,CATH,", "\nDM,CATHETER,"),
        )
        .unwrap();
        let header = dm.lines().next().unwrap();
        let first = dm.lines().nth(1).unwrap().edit("CATH-UCSD-0001", "C-1");
        fs::write(temp.0.join("suppdm.csv"), format!("{header}\n{first}\n")).unwrap();
        let project = temp.0.join("project.yaml");
        let text = fs::read_to_string(&project).unwrap().edit(
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
}
