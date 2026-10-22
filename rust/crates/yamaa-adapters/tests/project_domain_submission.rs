#![cfg(feature = "yaml")]
//! Project contracts: domain submission.
mod project {
    mod domain_submission {
        use crate::support::issues::assert_issues;
        use crate::support::ports::NoFunctions;
        use crate::support::Edit as _;
        use std::{fs, path::Path};
        use yamaa_adapters::{file_application, specification_report::Identity};
        use yamaa_core::application_observer::Scope;
        use yamaa_core::domain_entry::Request;
        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let root = crate::support::Scratch::new("domain-submission");
                Self(root.resolved(std::fs::canonicalize))
            }
            fn check(&self, raw: &str) -> Vec<yamaa_core::diagnostic::Diagnostic> {
                fs::write(self.0.join("spec.yaml"), raw).unwrap();
                let path = self.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                file_application::check(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(path),
                )
            }
        }
        fn identity(path: &str) -> Identity<'_> {
            crate::support::study::identity("submission", path)
        }
        fn copy(source: &Path, target: &Path) {
            fs::create_dir_all(target).unwrap();
            for entry in fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                let path = target.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy(&entry.path(), &path)
                } else {
                    fs::copy(entry.path(), path).unwrap();
                }
            }
        }
        /// An SDTM column's submission metadata: a 1.0 specification that
        /// declares it names its model (SUB-3), which requires the origin's
        /// source (SUB-15).
        const VALID: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - name: VALUE\n    type: str\n    derivation: SRC.VALUE\n    submission: {length: 3, origins: [{type: Assigned, source: Sponsor}]}\n";
        /// Each benchmark runs as it declares, with the project file that
        /// binds its codelists (CT-13): sdtm-lb-metadata's entry is its
        /// SDTM-complete layer under its own project.
        #[test]
        fn submission_fields_do_not_change_any_original_domain_artifact() {
            let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            for (example, entry, project, artifact) in [
                ("sdtm-ae-coding", "spec.yaml", "project.yaml", "ae.csv"),
                ("sdtm-dm-metadata", "spec.yaml", "project.yaml", "dm.csv"),
                (
                    "sdtm-lb-metadata",
                    "submission-spec.yaml",
                    "submission-project.yaml",
                    "lb.csv",
                ),
                (
                    "sdtm-vs-epoch-from-subject-elements",
                    "spec.yaml",
                    "project.yaml",
                    "vs.csv",
                ),
            ] {
                let case = repository.join("benchmarks").join(example);
                let study = Study::new();
                copy(&case, &study.0);
                let path = study.0.join(entry);
                let project = study.0.join(project);
                let mut run = project_domain(path.to_str().unwrap(), project.to_str().unwrap());
                assert!(run.issues().is_empty(), "{example}: {:?}", run.issues());
                run.save().unwrap();
                assert_eq!(
                    fs::read(study.0.join(artifact)).unwrap(),
                    fs::read(case.join("expected").join(artifact)).unwrap(),
                    "{example}"
                );
            }
        }
        /// Drop one YAML mapping entry: its header line and every following
        /// line indented deeper than the header, or written at its depth as
        /// an entry of the header's block sequence.
        fn without_entry(text: &str, header: &str) -> String {
            let depth = |line: &str| line.len() - line.trim_start().len();
            let mut kept = String::new();
            let mut found = false;
            let mut inside = false;
            for line in text.split_inclusive('\n') {
                if line.trim_end() == header {
                    (found, inside) = (true, true);
                    continue;
                }
                if inside
                    && (line.trim().is_empty()
                        || depth(line) > depth(header)
                        || (depth(line) == depth(header) && line.trim_start().starts_with("- ")))
                {
                    continue;
                }
                inside = false;
                kept.push_str(line);
            }
            assert!(found, "{header:?}");
            kept
        }
        /// Benchmarks pass a study-level case's root project, so its codelists
        /// apply. A run needs none of the project's study or the section's
        /// standards and define, so a specs-only section changes nothing.
        #[test]
        fn study_projects_with_or_without_submission_metadata_reproduce_each_domain_artifact() {
            let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
            for (example, artifact) in [
                ("sdtm-ae-coding", "ae.csv"),
                ("sdtm-dm-metadata", "dm.csv"),
                ("sdtm-vs-epoch-from-subject-elements", "vs.csv"),
            ] {
                for specs_only in [false, true] {
                    let case = repository.join("benchmarks").join(example);
                    let study = Study::new();
                    copy(&case, &study.0);
                    let project = study.0.join("project.yaml");
                    if specs_only {
                        let mut text = fs::read_to_string(&project).unwrap();
                        for header in ["study:", "  standards:", "  define: define.xml"] {
                            text = without_entry(&text, header);
                        }
                        fs::write(&project, text).unwrap();
                    }
                    let path = study.0.join("spec.yaml");
                    let path = path.to_str().unwrap();
                    let mut run = project_domain(path, project.to_str().unwrap());
                    assert!(run.issues().is_empty(), "{example}: {:?}", run.issues());
                    assert!(run.save().unwrap());
                    assert_eq!(
                        fs::read(study.0.join(artifact)).unwrap(),
                        fs::read(case.join("expected").join(artifact)).unwrap(),
                        "{example}, specs only: {specs_only}"
                    );
                }
            }
        }
        #[test]
        fn metadata_admission_has_exact_causes_before_study_access() {
            let study = Study::new();
            assert!(study.check(VALID).is_empty());
            let issues = study.check(&format!("{VALID}\nmetadata: {{label: forbidden}}\n"));
            assert_issues(
                &issues,
                &[
                    yamaa_adapters::json!({"phase":"validation","condition":"reserved_metadata_key","rule":"SUB-2","spec_paths":["metadata.label"],"context":{"key":"label","level":"root"}}),
                ],
                "a reserved metadata key",
            );
            let issues =
                study.check(&VALID.edit("length: 3", "length: -99999999999999999999999999999"));
            assert_eq!(issues[0].condition(), "submission_length_invalid");
            assert_eq!(issues[0].rule(), Some("SUB-7"));
            assert_eq!(
                yamaa_adapters::json_wire::encode_object(&issues[0].context),
                "{\"column\":\"VALUE\",\"length\":-99999999999999999999999999999}"
            );
            // A bare source admits Derived, which still needs its method.
            let issues = study.check(&VALID.edit(
                "{type: Assigned, source: Sponsor}",
                "{type: Derived, source: Sponsor}",
            ));
            assert_eq!(
                issues
                    .iter()
                    .map(|issue| issue.condition())
                    .collect::<Vec<_>>(),
                ["method_missing"]
            );
            let raw = VALID.edit(
                "submission: {length: 3",
                "verifications: [{max_length: {max: 4}}]\n    submission: {length: 3",
            );
            let issues = study.check(&raw);
            assert_eq!(issues[0].condition(), "declared_length_conflict");
            assert_eq!(
                yamaa_adapters::json_wire::encode_object(&issues[0].context),
                "{\"column\":\"VALUE\",\"length\":3,\"max_length\":4}"
            );
            assert!(!study.0.join("output.csv").exists());
        }
        /// One closed CDISC vocabulary, in the order of the data file that
        /// holds them all (P-VOC-1).
        fn vocabulary(name: &str) -> Vec<String> {
            use yamaa_core::schema::DocumentNode as N;
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join("yaml/vocabulary/cdisc.yaml");
            let bytes = fs::read(path).unwrap();
            let decoded = yamaa_adapters::yaml_decode::decode_yaml(&bytes).unwrap();
            let document = &decoded.document;
            let entries = document
                .field(document.root(), "vocabularies")
                .and_then(|vocabularies| document.field(vocabularies, name))
                .and_then(|vocabulary| document.field(vocabulary, "values"))
                .unwrap_or_else(|| panic!("{name} has no values"));
            let N::Sequence(items) = &document.nodes()[entries] else {
                panic!("{name}: values is not a list")
            };
            items
                .iter()
                .map(|&item| match document.field(item, "value") {
                    Some(value) => match &document.nodes()[value] {
                        N::Text(text) => text.clone(),
                        other => panic!("{name}: {other:?}"),
                    },
                    None => panic!("{name}: an entry has no value"),
                })
                .collect()
        }
        /// The engine's typed copies of each vocabulary are the data file's
        /// lists, in order. The 0.5 specification model admits `core` and
        /// `data_type` from `Core::NAMES` and `DataType::NAMES`. The tests
        /// below reach the lists a reader or the Define-XML writer keeps to
        /// itself. The engine keeps no list of where comparators: the 1.0
        /// lowering reads the two the 1.0 schema spells, NE and NOTIN, by name.
        #[test]
        fn engine_vocabularies_equal_the_cdisc_vocabulary_file() {
            use yamaa_core::specification_submission_document::{
                Core, DataType, DatasetClass, DatasetSubclass, OriginSource, OriginType, ROLES,
            };
            for (name, engine) in [
                ("dataset_class", DatasetClass::NAMES),
                ("dataset_subclass", DatasetSubclass::NAMES),
                ("standard_name", yamaa_core::ast::STANDARD_NAMES),
                ("origin_type", OriginType::NAMES),
                ("origin_source", OriginSource::NAMES),
                ("role", ROLES),
                ("core_designation", Core::NAMES),
                ("submission_data_type", DataType::NAMES),
            ] {
                assert_eq!(vocabulary(name), engine, "{name}");
            }
            // SUB-26 names each date or time form by its data type.
            let data_types = vocabulary("submission_data_type");
            for form in yamaa_core::iso8601::Form::ALL {
                assert!(
                    data_types.iter().any(|name| name == form.name()),
                    "{form:?}"
                );
            }
        }
        /// P-MODEL-1: the 1.0 reader lowers each model the data file lists
        /// to a kind of its own, reaches every kind, and lowers no other
        /// spelling. `ModelKind` has no spellings; the lowering holds them.
        #[test]
        fn the_1_0_reader_lowers_exactly_the_models_the_file_lists() {
            use yamaa_core::ast::ModelKind;
            let lowered = |model: &str| {
                let text = format!(
                    "yamaa: '1.0'\ntype: spec\nmodel: {model}\ndomain: TEST\nkeys: [ID]\n\
                     input: {{SRC: input.csv}}\noutput: {{path: output.csv, columns: [ID]}}\n\
                     columns:\n  - {{name: ID, type: int, derivation: SRC.ID}}\n"
                );
                let decoded = yamaa_adapters::yaml_decode::decode_yaml(text.as_bytes()).unwrap();
                yamaa_core::front_end::v1::spec::lower(&decoded.document, None)
                    .into_spec()
                    .model
                    .map(|model| model.kind)
            };
            // Every kind, in declaration order: a new kind fails this match
            // until it is listed, and then the file must list its model.
            let kinds = [ModelKind::Sdtm, ModelKind::Send, ModelKind::Adam];
            kinds.iter().for_each(|kind| match kind {
                ModelKind::Sdtm | ModelKind::Send | ModelKind::Adam => {}
            });
            let models = vocabulary("model");
            assert_eq!(
                models
                    .iter()
                    .map(|model| lowered(model))
                    .collect::<Vec<_>>(),
                kinds.map(Some)
            );
            for other in ["sdtm", "ADAM", "Send", "SDTMIG"] {
                assert_eq!(lowered(other), None, "{other}");
            }
        }
        /// P-STD-1: `GuideStatus` names each implementation guide status the
        /// data file lists, and the 1.0 project reader lowers each to the
        /// status of that name and refuses any other. The 0.5 reader lists
        /// Draft and Final alone, and the file compares only the 1.0 schema
        /// set (`schemas: [yaml/v1]`).
        #[test]
        fn the_1_0_reader_admits_exactly_the_guide_statuses_the_file_lists() {
            use yamaa_core::ast::GuideStatus;
            // Every status, in declaration order: a new status fails this
            // match until it is listed, and then the file must list it.
            let every = [
                GuideStatus::Draft,
                GuideStatus::Final,
                GuideStatus::Provisional,
            ];
            every.iter().for_each(|status| match status {
                GuideStatus::Draft | GuideStatus::Final | GuideStatus::Provisional => {}
            });
            let statuses = vocabulary("standard_status");
            assert_eq!(statuses, every.map(GuideStatus::name));
            let lowered = |status: &str| {
                let text = format!(
                    "yamaa: '1.0'\ntype: project\nsdtm:\n  specs: [spec.yaml]\n  standards:\n    \
                     - {{name: SDTMIG, version: '3.4', status: {status}}}\n"
                );
                let decoded = yamaa_adapters::yaml_decode::decode_yaml(text.as_bytes()).unwrap();
                yamaa_core::front_end::v1::project::project(&decoded.document, None)
                    .unwrap()
                    .submission
                    .ok()
                    .map(|declarations| declarations.sections[0].standards[0].status)
            };
            for (status, kind) in statuses.iter().zip(every) {
                assert_eq!(lowered(status), Some(kind), "{status}");
            }
            for other in ["draft", "FINAL", "Withdrawn"] {
                assert_eq!(lowered(other), None, "{other}");
            }
        }
        /// CT-2: the Define-XML writer, whose own facet guard closes the
        /// publishing set again after the project is admitted, writes every
        /// publishing set the data file lists.
        #[test]
        fn the_define_xml_writer_writes_every_publishing_set_the_file_lists() {
            use yamaa_adapters::file_define::Define;
            let case = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join("benchmarks/sdtm-ta-sponsor-epoch");
            for set in vocabulary("publishing_set") {
                let study = Study::new();
                copy(&case, &study.0);
                // The document reads the table the domain run published.
                fs::copy(case.join("expected/ta.csv"), study.0.join("ta.csv")).unwrap();
                let project = study.0.join("project.yaml");
                let text = fs::read_to_string(&project).unwrap();
                assert!(text.contains("publishing_set: SDTM\n"));
                fs::write(
                    &project,
                    text.edit(
                        "publishing_set: SDTM\n",
                        &format!("publishing_set: {set}\n"),
                    ),
                )
                .unwrap();
                let define = Define::prepare(
                    project.to_str().unwrap(),
                    yamaa_core::project_environment::Submission::Sdtm,
                    Some("2026-09-11T00:00:00"),
                    "0.0.0",
                    Some(yamaa_core::project_function::Language::Python),
                    None,
                );
                assert!(define.issues().is_empty(), "{set}: {:?}", define.issues());
                let (_, xml, _) = define
                    .artifacts()
                    .find(|(written, ..)| *written == "define.xml")
                    .unwrap_or_else(|| panic!("{set}: no define.xml"));
                let xml = std::str::from_utf8(xml).unwrap();
                assert!(xml.contains(&format!("PublishingSet=\"{set}\"")), "{set}");
            }
        }
        /// SUB-9: every role the data file lists is admitted where the
        /// specification is read, and any other fails there.
        #[test]
        fn a_role_outside_the_nine_fails_where_the_specification_is_read() {
            let study = Study::new();
            let with_role =
                |role: &str| VALID.edit("submission: {", &format!("submission: {{role: {role}, "));
            let roles = vocabulary("role");
            assert_eq!(roles.len(), 9);
            for role in &roles {
                assert!(study.check(&with_role(role)).is_empty(), "{role}");
            }
            for role in ["Qualifier", "record qualifier", "Analysis Value"] {
                let issues = study.check(&with_role(role));
                assert_eq!(issues.len(), 1, "{role}: {issues:?}");
                assert_eq!(
                    (issues[0].phase(), issues[0].condition(), issues[0].rule()),
                    ("validation", "value_not_permitted", Some("SCHEMA-12"))
                );
                assert_eq!(issues[0].spec_paths, ["columns.VALUE.submission.role"]);
                let context = yamaa_core::json::Value::Object(issues[0].context.clone());
                assert_eq!(context["value"], role);
                let permitted = context["permitted"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_str().unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(roles, permitted);
            }
            assert!(!study.0.join("output.csv").exists());
        }
        /// CT-2: a terminology standard's name is one of the standard names
        /// the data file lists, refused where the project is read, before any
        /// document is generated.
        #[test]
        fn a_terminology_standard_name_outside_the_vocabulary_fails_where_the_project_is_read() {
            let study = Study::new();
            fs::write(study.0.join("spec.yaml"), VALID).unwrap();
            let check = |name: &str| {
                fs::write(
                    study.0.join("project.yaml"),
                    format!(
                        "yamaa: \"1.0\"\ntype: project\ncodelists:\n  - standard: {{name: {name}, \
                         publishing_set: SDTM, version: '2026-03-27'}}\n    codelists:\n      \
                         - {{id: SEX, name: Sex, items: [{{value: F}}, {{value: M}}]}}\nsdtm:\n  \
                         specs: [spec.yaml]\n"
                    ),
                )
                .unwrap();
                let path = study.0.join("spec.yaml");
                let project = study.0.join("project.yaml");
                file_application::check(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path.to_str().unwrap(),
                        environment: Some(project.to_str().unwrap()),
                    },
                    identity(path.to_str().unwrap()),
                )
            };
            let names = vocabulary("standard_name");
            for name in &names {
                assert!(check(name).is_empty(), "{name}");
            }
            let issues = check("CDISC CT");
            assert_eq!(issues.len(), 1, "{issues:?}");
            assert_eq!(
                (issues[0].phase(), issues[0].condition(), issues[0].rule()),
                ("validation", "value_not_permitted", Some("SCHEMA-12"))
            );
            assert_eq!(issues[0].spec_paths, ["codelists[0].standard.name"]);
            let context = yamaa_core::json::Value::Object(issues[0].context.clone());
            assert_eq!(context["value"], "CDISC CT");
            let permitted = context["permitted"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(names, permitted);
            assert!(!study.0.join("output.csv").exists());
        }
        #[test]
        fn declared_length_is_checked_once_over_unicode_scalar_values() {
            let study = Study::new();
            fs::write(study.0.join("spec.yaml"), VALID).unwrap();
            fs::write(study.0.join("input.csv"), "ID,VALUE\n1,é字😀\n2,abcd\n").unwrap();
            let path = study.0.join("spec.yaml");
            let path = path.to_str().unwrap();
            let mut run = crate::support::study::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: path,
                    environment: None,
                },
                identity(path),
            )
            .unwrap();
            let issues = run.issues();
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].condition(), "length_failed");
            assert_eq!(issues[0].rule(), Some("CHECK-8"));
            assert_eq!(
                yamaa_adapters::json_wire::encode_object(&issues[0].context),
                "{\"column\":\"VALUE\",\"failure_count\":1,\"keys\":[{\"ID\":2}],\"max\":3}"
            );
            assert!(run.save().is_err());
            assert!(!study.0.join("output.csv").exists());
        }

        /// SUB-26: a `str` column's date or time data type is a form each
        /// present completed value must have; an entry's data type decides
        /// the rows its where clause selects, and the column's every other row.
        #[test]
        fn date_and_time_data_types_check_each_completed_value_against_their_form() {
            fn domain(study: &Study, spec: &str, input: &str) -> file_application::Domain {
                fs::write(study.0.join("spec.yaml"), spec).unwrap();
                fs::write(study.0.join("input.csv"), input).unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(path),
                )
                .unwrap()
            }
            const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: LB\nkeys: [ID, LBTESTCD]\ninput: {SRC: input.csv}\noutput: {path: output.csv, report: checks.csv, columns: [ID, LBTESTCD, LBDTC, LBORRES]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: LBTESTCD, type: str}\n  - {name: LBDTC, type: str, derivation: SRC.DTC, submission: {data_type: DTC_TYPE, origins: [{type: Collected, source: Investigator}]}}\n  - {name: LBORRES, type: str, derivation: SRC.RES, submission: {length: 20, origins: [{type: Collected, source: Investigator}], value_level: [{id: ONSET, where: {LBTESTCD: [ONSET]}, data_type: date}]}}\nrows:\n  - id: glucose\n    derivations: {LBTESTCD: \"'GLUC'\"}\n  - id: onset\n    derivations: {LBTESTCD: \"'ONSET'\"}\n";
            let study = Study::new();
            // The column's type checks every row: a space is not ISO 8601.
            let run = domain(
                &study,
                &SPEC.edit("DTC_TYPE", "partialDatetime"),
                "ID,DTC,RES\n1,2025-03-04T08:30,2025-03-04\n2,2025-03-04 08:30,2025-03-05\n",
            );
            assert_eq!(run.issues().len(), 1, "{:?}", run.issues());
            let issue = &run.issues()[0];
            assert_eq!(
                (issue.phase(), issue.condition(), issue.rule()),
                ("verification", "data_type_failed", Some("SUB-26"))
            );
            assert_eq!(issue.spec_paths, ["columns.LBDTC.submission.data_type"]);
            assert_eq!(
                yamaa_core::json::Value::Object(issue.context.clone()),
                yamaa_adapters::json!({"column": "LBDTC", "data_type": "partialDatetime", "failure_count": 2, "keys": [{"ID": 2, "LBTESTCD": "GLUC"}, {"ID": 2, "LBTESTCD": "ONSET"}]})
            );
            // The run report names the check `data_type` (CHECK-24).
            let mut run = run;
            assert!(run.save().is_err());
            let checks = fs::read_to_string(study.0.join("checks.csv")).unwrap();
            assert!(
                checks.contains(
                    "output.csv,check,columns.LBDTC.submission.data_type,data_type,,LBDTC,SUB-26,error,violated,data_type_failed,2,4,"
                ),
                "{checks}"
            );
            fs::remove_file(study.0.join("checks.csv")).unwrap();
            // An entry's type checks only the rows its where clause selects:
            // the glucose result is text, while the onset result must be a
            // date.
            let run = domain(
                &study,
                &SPEC.edit("DTC_TYPE", "partialDatetime"),
                "ID,DTC,RES\n1,2025-03-04T08:30,5.1\n2,2025-03,2025-03\n",
            );
            assert_eq!(run.issues().len(), 1, "{:?}", run.issues());
            let issue = &run.issues()[0];
            assert_eq!(issue.condition(), "data_type_failed");
            assert_eq!(
                issue.spec_paths,
                ["columns.LBORRES.submission.value_level.ONSET.data_type"]
            );
            assert_eq!(
                yamaa_core::json::Value::Object(issue.context.clone()),
                yamaa_adapters::json!({"column": "LBORRES", "data_type": "date", "failure_count": 2, "keys": [{"ID": 1, "LBTESTCD": "ONSET"}, {"ID": 2, "LBTESTCD": "ONSET"}]})
            );
            // Every value of its form, and a missing one, passes and saves.
            let mut run = domain(
                &study,
                &SPEC.edit("DTC_TYPE", "incompleteDatetime"),
                "ID,DTC,RES\n1,2025---04T08,2025-03-04\n2,,2025-03-05\n",
            );
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            assert!(run.save().unwrap());
            assert_eq!(
                fs::read_to_string(study.0.join("output.csv")).unwrap(),
                "ID,LBTESTCD,LBDTC,LBORRES\n1,GLUC,2025---04T08,2025-03-04\n2,GLUC,,2025-03-05\n1,ONSET,2025---04T08,2025-03-04\n2,ONSET,,2025-03-05\n"
            );
            let checks = fs::read_to_string(study.0.join("checks.csv")).unwrap();
            for row in [
                "output.csv,check,columns.LBDTC.submission.data_type,data_type,,LBDTC,SUB-26,error,held,,0,4,,{}\n",
                "output.csv,check,columns.LBORRES.submission.value_level.ONSET.data_type,data_type,,LBORRES,SUB-26,error,held,,0,2,,{}\n",
            ] {
                assert!(checks.contains(row), "{row}\n{checks}");
            }
            // An entry selects its rows by comparing text, so its where
            // column must be a declared `str` column (SUB-12).
            let issues = study.check(
                &SPEC
                    .edit("DTC_TYPE", "partialDatetime")
                    .edit("{name: LBTESTCD, type: str}", "{name: LBTESTCD, type: int}")
                    .edit("\"'GLUC'\"", "\"'1'\"")
                    .edit("\"'ONSET'\"", "\"'2'\""),
            );
            assert_eq!(issues.len(), 1, "{issues:?}");
            assert_eq!(
                (issues[0].condition(), issues[0].rule()),
                ("value_level_where_invalid", Some("SUB-12"))
            );
            assert_eq!(
                issues[0].spec_paths,
                ["columns.LBORRES.submission.value_level.ONSET.where.LBTESTCD"]
            );
        }

        /// A value-level check reads its completed where column. When that
        /// column is not a key and a row template leaves it to its column
        /// derivation, it completes after the checked column, so the check
        /// waits for it in the executor, the resolved plan and its reference
        /// order alike: `check()` passes and the run succeeds.
        #[test]
        fn value_level_checks_wait_for_a_where_column_the_column_phase_completes() {
            const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: LB\nkeys: [ID, SEQ]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, SEQ, LBORRES, LBTESTCD]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: SEQ, type: int, derivation: SRC.SEQ}\n  - {name: LBORRES, type: str, derivation: SRC.RES, submission: {length: 20, origins: [{type: Collected, source: Investigator}], value_level: [{id: ONSET, where: {LBTESTCD: [ONSET]}, ENTRY}]}}\n  - {name: LBTESTCD, type: str, derivation: SRC.CODE}\nrows:\n  - id: onset\n    filter: \"SRC.CODE = 'ONSET'\"\n    derivations: {LBTESTCD: \"'ONSET'\"}\n  - id: other\n    filter: \"SRC.CODE <> 'ONSET'\"\n";
            for (entry, field, failing, context) in [
                (
                    "mandatory: true",
                    "mandatory",
                    "ID,SEQ,CODE,RES\n1,1,ONSET,\n1,2,GLUC,\n",
                    yamaa_adapters::json!({"column": "LBORRES", "failure_count": 1, "keys": [{"ID": 1, "SEQ": 1}]}),
                ),
                (
                    "data_type: date",
                    "data_type",
                    "ID,SEQ,CODE,RES\n1,1,ONSET,2025-03\n1,2,GLUC,2025-03\n",
                    yamaa_adapters::json!({"column": "LBORRES", "data_type": "date", "failure_count": 1, "keys": [{"ID": 1, "SEQ": 1}]}),
                ),
            ] {
                let study = Study::new();
                let spec = SPEC.edit("ENTRY", entry);
                let issues = study.check(&spec);
                assert!(issues.is_empty(), "{entry}: {issues:?}");
                // The onset row holds a date; the glucose row, which the
                // entry does not select, may hold anything or nothing.
                let mut run = domain(
                    &study,
                    &spec,
                    "ID,SEQ,CODE,RES\n1,1,ONSET,2025-03-04\n1,2,GLUC,\n",
                );
                assert!(run.issues().is_empty(), "{entry}: {:?}", run.issues());
                assert!(run.save().unwrap());
                assert_eq!(
                    fs::read_to_string(study.0.join("output.csv")).unwrap(),
                    "ID,SEQ,LBORRES,LBTESTCD\n1,1,2025-03-04,ONSET\n1,2,,GLUC\n"
                );
                // The check selects the onset row by its completed test code.
                let run = domain(&study, &spec, failing);
                assert_eq!(run.issues().len(), 1, "{entry}: {:?}", run.issues());
                let issue = &run.issues()[0];
                assert_eq!(
                    issue.spec_paths,
                    [format!(
                        "columns.LBORRES.submission.value_level.ONSET.{field}"
                    )]
                );
                assert_eq!(
                    yamaa_core::json::Value::Object(issue.context.clone()),
                    context,
                    "{entry}"
                );
            }
        }

        /// SUB-27: a dataset of a tabulation class carries no `datetime`
        /// output column; an internal one, an analysis class and a dataset
        /// without a class are not refused.
        #[test]
        fn tabulation_datasets_refuse_a_datetime_output_column() {
            const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: VS\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, OUTPUT]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: VSDTM, type: datetime, derivation: SRC.DTC}\n  - {name: OUTPUT, type: str, derivation: SRC.DTC}\nsubmission: {label: Vital Signs, class: CLASS, structure: One record per reading, repeating: true}\n";
            let study = Study::new();
            let issues = study.check(&SPEC.edit("OUTPUT]", "VSDTM]").edit("CLASS", "FINDINGS"));
            assert_issues(
                &issues,
                &[
                    yamaa_adapters::json!({"phase":"validation","condition":"value_not_permitted","rule":"SUB-27","spec_paths":["columns.VSDTM.type"],"context":{"column":"VSDTM","value":"datetime","class":"FINDINGS","permitted":["str","date"]}}),
                ],
                "a datetime output column in a tabulation dataset",
            );
            for spec in [
                SPEC.edit("CLASS", "FINDINGS"),
                SPEC.edit("OUTPUT]", "VSDTM]")
                    .edit("model: SDTM", "model: ADaM")
                    .edit("CLASS", "BASIC DATA STRUCTURE"),
                SPEC.edit("OUTPUT]", "VSDTM]").edit(
                    "submission: {label: Vital Signs, class: CLASS, structure: One record per reading, repeating: true}\n",
                    "",
                ),
            ] {
                let issues = study.check(&spec);
                assert!(issues.is_empty(), "{spec}\n{issues:?}");
            }
        }

        /// SUB-23 (D25): the rules that read the model a 1.0 specification
        /// declares are checked on the specification alone, each where it is
        /// declared (SUB-5, SUB-6, SUB-9, SUB-10, SUB-15, SUB-25). `core` and
        /// ADaM's `mandatory` are required only with dataset metadata, and a
        /// 0.5 specification declares no model, so these rules wait for a
        /// document there.
        #[test]
        fn rules_that_read_the_model_are_checked_on_the_specification_alone() {
            const ADAM: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: ADaM\ndomain: ADSL\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, CODE, VALUE, DAY]}\nsubmission: {label: Subject Level, class: SUBJECT LEVEL ANALYSIS DATASET, structure: One record per subject, repeating: false}\ncolumns:\n  - name: ID\n    type: int\n    label: Identifier\n    derivation: SRC.ID\n    submission: {core: Req, mandatory: true, length: 8, origins: [{type: Assigned}]}\n  - name: CODE\n    type: str\n    label: Code\n    derivation: SRC.CODE\n    submission: {core: Perm, mandatory: false, length: 4, origins: [{type: Assigned}]}\n  - name: VALUE\n    type: str\n    label: Value\n    derivation: SRC.VALUE\n    submission: {core: Perm, mandatory: false, length: 3, origins: [{type: Assigned}], value_level: [{id: A, where: {CODE: [A]}, core: Req}]}\n  - name: DAY\n    type: date\n    label: Day\n    derivation: SRC.DAY\n    submission: {core: Perm, mandatory: false, origins: [{type: Assigned}]}\n";
            const SDTM: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: DM\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, VALUE, DAY]}\nsubmission: {label: Demographics, class: SPECIAL PURPOSE, structure: One record per subject, repeating: false}\ncolumns:\n  - name: ID\n    type: int\n    label: Identifier\n    derivation: SRC.ID\n    submission: {core: Req, length: 8, origins: [{type: Assigned, source: Sponsor}]}\n  - name: VALUE\n    type: str\n    label: Value\n    derivation: SRC.VALUE\n    submission: {core: Perm, length: 3, origins: [{type: Collected, source: Investigator}]}\n  - name: DAY\n    type: date\n    label: Day\n    derivation: SRC.DAY\n    submission: {core: Perm, origins: [{type: Assigned, source: Sponsor}]}\n";
            let study = Study::new();
            for spec in [ADAM, SDTM] {
                let issues = study.check(spec);
                assert!(issues.is_empty(), "{spec}\n{issues:?}");
            }
            let value = |json: &str| yamaa_adapters::json_wire::decode(json).unwrap();
            // A value-level selector names a text column (SUB-12).
            let sdtm_text_key = SDTM.replacen("type: int", "type: str", 1);
            for (base, from, to, condition, rule, path, context) in [
                // A designation the model does not publish (SUB-10).
                (
                    ADAM,
                    "{core: Perm, mandatory: false, length: 3",
                    "{core: Exp, mandatory: false, length: 3",
                    "core_mandatory_conflict",
                    "SUB-10",
                    "columns.VALUE.submission.core",
                    r#"{"column": "VALUE", "reason": "core_not_admitted", "core": "Exp", "model": "ADaM"}"#,
                ),
                (
                    ADAM,
                    "value_level: [{id: A, where: {CODE: [A]}, core: Req}]",
                    "value_level: [{id: A, where: {CODE: [A]}, core: Exp}]",
                    "core_mandatory_conflict",
                    "SUB-10",
                    "columns.VALUE.submission.value_level.A.core",
                    r#"{"column": "VALUE", "entry": "A", "reason": "core_not_admitted", "core": "Exp", "model": "ADaM"}"#,
                ),
                (
                    SDTM,
                    "{core: Perm, length: 3",
                    "{core: Cond, length: 3",
                    "core_mandatory_conflict",
                    "SUB-10",
                    "columns.VALUE.submission.core",
                    r#"{"column": "VALUE", "reason": "core_not_admitted", "core": "Cond", "model": "SDTM"}"#,
                ),
                // ADaM declares `mandatory`; SDTM never relaxes `Req`.
                (
                    ADAM,
                    "{core: Perm, mandatory: false, length: 3",
                    "{core: Perm, length: 3",
                    "core_mandatory_conflict",
                    "SUB-10",
                    "columns.VALUE.submission",
                    r#"{"column": "VALUE", "reason": "mandatory_missing", "model": "ADaM"}"#,
                ),
                (
                    SDTM,
                    "{core: Perm, length: 3",
                    "{core: Req, mandatory: false, length: 3",
                    "core_mandatory_conflict",
                    "SUB-10",
                    "columns.VALUE.submission.mandatory",
                    r#"{"column": "VALUE", "reason": "required_not_mandatory", "model": "SDTM"}"#,
                ),
                // An entry can inherit either half of the conflict.
                (
                    sdtm_text_key.as_str(),
                    "{core: Perm, length: 3, origins: [{type: Collected, source: Investigator}]}",
                    "{core: Perm, mandatory: false, length: 3, origins: [{type: Collected, source: Investigator}], value_level: [{id: A, where: {ID: ['1']}, core: Req}]}",
                    "core_mandatory_conflict",
                    "SUB-10",
                    "columns.VALUE.submission.value_level.A.core",
                    r#"{"column": "VALUE", "entry": "A", "reason": "required_not_mandatory", "model": "SDTM"}"#,
                ),
                (
                    sdtm_text_key.as_str(),
                    "{core: Perm, length: 3, origins: [{type: Collected, source: Investigator}]}",
                    "{core: Req, length: 3, origins: [{type: Collected, source: Investigator}], value_level: [{id: A, where: {ID: ['1']}, mandatory: false}]}",
                    "core_mandatory_conflict",
                    "SUB-10",
                    "columns.VALUE.submission.value_level.A.mandatory",
                    r#"{"column": "VALUE", "entry": "A", "reason": "required_not_mandatory", "model": "SDTM"}"#,
                ),
                // A key's existing admission reports the conflict once.
                (
                    SDTM,
                    "{core: Req, length: 8",
                    "{core: Req, mandatory: false, length: 8",
                    "value_not_permitted",
                    "SUB-10",
                    "columns.ID.submission.mandatory",
                    r#"{"column": "ID"}"#,
                ),
                // ADaM has no role and no domain (SUB-9, SUB-5).
                (
                    ADAM,
                    "{core: Perm, mandatory: false, length: 3",
                    "{core: Perm, mandatory: false, role: Topic, length: 3",
                    "value_not_permitted",
                    "SUB-5",
                    "columns.VALUE.submission.role",
                    r#"{"column": "VALUE", "value": "Topic", "model": "ADaM"}"#,
                ),
                (
                    ADAM,
                    "repeating: false}",
                    "repeating: false, domain: ADSL}",
                    "value_not_permitted",
                    "SUB-5",
                    "submission.domain",
                    r#"{"value": "ADSL", "model": "ADaM"}"#,
                ),
                // The class belongs to the model (SUB-10).
                (
                    ADAM,
                    "class: SUBJECT LEVEL ANALYSIS DATASET",
                    "class: SPECIAL PURPOSE",
                    "value_not_permitted",
                    "SUB-10",
                    "submission.class",
                    r#"{"value": "SPECIAL PURPOSE", "model": "ADaM"}"#,
                ),
                (
                    SDTM,
                    "class: SPECIAL PURPOSE",
                    "class: BASIC DATA STRUCTURE",
                    "value_not_permitted",
                    "SUB-10",
                    "submission.class",
                    r#"{"value": "BASIC DATA STRUCTURE", "model": "SDTM"}"#,
                ),
                // Origin pairs (SUB-15).
                (
                    ADAM,
                    "length: 3, origins: [{type: Assigned}]",
                    "length: 3, origins: [{type: Collected, source: Subject}]",
                    "origin_source_not_admitted",
                    "SUB-15",
                    "columns.VALUE.submission.origins[0]",
                    r#"{"column": "VALUE", "origin_type": "Collected", "origin_source": "Subject", "model": "ADaM"}"#,
                ),
                (
                    SDTM,
                    "origins: [{type: Collected, source: Investigator}]",
                    "origins: [{type: Collected}]",
                    "origin_source_not_admitted",
                    "SUB-15",
                    "columns.VALUE.submission.origins[0]",
                    r#"{"column": "VALUE", "origin_type": "Collected", "model": "SDTM"}"#,
                ),
                // A date is submitted as the one type its model gives it
                // (SUB-6), with ADaM's fixed display format (SUB-25).
                (
                    ADAM,
                    "submission: {core: Perm, mandatory: false, origins",
                    "submission: {core: Perm, mandatory: false, data_type: date, origins",
                    "submission_data_type_not_admitted",
                    "SUB-6",
                    "columns.DAY.submission",
                    r#"{"column": "DAY", "declared_type": "date", "data_type": "date", "model": "ADaM"}"#,
                ),
                // An entry that inherits the type leaves it to its column.
                (
                    ADAM,
                    "submission: {core: Perm, mandatory: false, origins: [{type: Assigned}]}\n",
                    "submission: {core: Perm, mandatory: false, data_type: date, origins: [{type: Assigned}], value_level: [{id: B, where: {CODE: [B]}, mandatory: true}]}\n",
                    "submission_data_type_not_admitted",
                    "SUB-6",
                    "columns.DAY.submission",
                    r#"{"column": "DAY", "declared_type": "date", "data_type": "date", "model": "ADaM"}"#,
                ),
                (
                    SDTM,
                    "submission: {core: Perm, origins: [{type: Assigned",
                    "submission: {core: Perm, data_type: integer, origins: [{type: Assigned",
                    "submission_data_type_not_admitted",
                    "SUB-6",
                    "columns.DAY.submission",
                    r#"{"column": "DAY", "declared_type": "date", "data_type": "integer", "model": "SDTM"}"#,
                ),
                (
                    ADAM,
                    "submission: {core: Perm, mandatory: false, origins",
                    "submission: {core: Perm, mandatory: false, display_format: YYMMDD10., origins",
                    "value_not_permitted",
                    "SUB-25",
                    "columns.DAY.submission.display_format",
                    r#"{"column": "DAY", "value": "YYMMDD10.", "permitted": "DATE9."}"#,
                ),
            ] {
                assert_eq!(base.matches(from).count(), 1, "{from}");
                let spec = base.replace(from, to);
                let issues = study.check(&spec);
                assert_issues(
                    &issues,
                    &[yamaa_adapters::json!({"phase":"validation","condition":condition,"rule":rule,"spec_paths":[path],"context":value(context)})],
                    to,
                );
            }
            // Without dataset metadata, `core` and ADaM's `mandatory` are not
            // yet required, while a declared value is still judged.
            let partial = ADAM.edit(
                "submission: {label: Subject Level, class: SUBJECT LEVEL ANALYSIS DATASET, structure: One record per subject, repeating: false}\n",
                "",
            );
            assert!(study
                .check(&partial.edit("{core: Perm, mandatory: false, length: 3", "{length: 3"))
                .is_empty());
            let issues = study.check(&partial.edit(
                "{core: Perm, mandatory: false, length: 3",
                "{core: Exp, length: 3",
            ));
            assert_eq!(issues.len(), 1, "{issues:?}");
            assert_eq!(issues[0].spec_paths, ["columns.VALUE.submission.core"]);
            // A 0.5 specification declares no model: its section decides
            // these rules when a document composes it. This case goes with
            // the 0.5 path.
            let valid_0_5 = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, VALUE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - name: VALUE\n    type: str\n    derivation: SRC.VALUE\n    submission: {length: 3, origin: {type: Assigned}}\n";
            assert!(study
                .check(&valid_0_5.edit("submission: {", "submission: {core: Cond, "))
                .is_empty());
            assert!(!study.0.join("output.csv").exists());
        }

        const ROW_CT: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: LB\nkeys: [ID, LBTESTCD]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, RESULT, LBTESTCD]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: RESULT, type: str, derivation: SRC.VALUE, submission: {origins: [{type: Assigned, source: Sponsor}], codelist: DEFAULT, value_level: [{id: GLUC, where: {LBTESTCD: [GLUC]}, codelist: GLUCOSE}]}}\n  - {name: LBTESTCD, type: str}\nrows:\n  - id: glucose\n    derivations: {LBTESTCD: \"'GLUC'\"}\n  - id: creatinine\n    derivations: {LBTESTCD: \"'CREAT'\"}\n";
        const CT_PROJECT: &str = "yamaa: \"1.0\"\ntype: project\ncodelists:\n  - codelists:\n      - {id: DEFAULT, name: Default values, items: [{value: D}]}\n      - {id: GLUCOSE, name: Glucose values, items: [{value: G}]}\n      - {id: EXT, name: Extended values, extensible: true, items: [{value: X}]}\n      - {id: DICT, name: Dictionary, external: {dictionary: MedDRA, version: '27.1'}}\n";
        #[test]
        fn per_value_codelists_inherit_origins_and_check_completed_where_columns() {
            let study = Study::new();
            fs::write(study.0.join("spec.yaml"), ROW_CT).unwrap();
            fs::write(study.0.join("project.yaml"), CT_PROJECT).unwrap();
            fs::write(study.0.join("input.csv"), "ID,VALUE\n1,G\n2,D\n3,\n").unwrap();
            let path = study.0.join("spec.yaml");
            let path = path.to_str().unwrap();
            let project = study.0.join("project.yaml");
            let mut run = project_domain(path, project.to_str().unwrap());
            assert_eq!(run.issues().len(), 2, "{:?}", run.issues());
            for issue in run.issues() {
                assert_eq!(
                    (issue.phase(), issue.condition(), issue.rule()),
                    ("verification", "allowed_values_failed", Some("CT-7"))
                );
                assert_eq!(issue.spec_paths, ["columns.RESULT.submission.codelist"]);
            }
            let contexts = run
                .issues()
                .iter()
                .map(|issue| yamaa_core::json::Value::Object(issue.context.clone()))
                .collect::<Vec<_>>();
            assert_eq!(
                contexts[0],
                yamaa_adapters::json!({"column":"RESULT", "codelist":"DEFAULT", "failure_count":1, "keys":[{"ID":1,"LBTESTCD":"CREAT"}], "values":["G"]})
            );
            assert_eq!(
                contexts[1],
                yamaa_adapters::json!({"column":"RESULT", "codelist":"GLUCOSE", "failure_count":1, "keys":[{"ID":2,"LBTESTCD":"GLUC"}], "values":["D"]})
            );
            assert!(run.save().is_err());
            assert!(!study.0.join("output.csv").exists());
        }
        #[test]
        fn per_value_extensible_lists_are_enforced_on_their_rows_and_dictionaries_replace_the_default(
        ) {
            // An extensible list of items states its rows' value set like any other
            // list: the glucose rows are checked against EXT alone.
            let study = Study::new();
            fs::write(
                study.0.join("spec.yaml"),
                ROW_CT.edit("codelist: GLUCOSE", "codelist: EXT"),
            )
            .unwrap();
            fs::write(study.0.join("project.yaml"), CT_PROJECT).unwrap();
            fs::write(study.0.join("input.csv"), "ID,VALUE\n1,D\n2,X\n").unwrap();
            let path = study.0.join("spec.yaml");
            let path = path.to_str().unwrap();
            let environment = study.0.join("project.yaml");
            let mut run = project_domain(path, environment.to_str().unwrap());
            assert_eq!(run.issues().len(), 2, "{:?}", run.issues());
            assert_eq!(
                (
                    run.issues()[0].phase(),
                    run.issues()[0].condition(),
                    run.issues()[0].rule()
                ),
                ("verification", "allowed_values_failed", Some("CT-7"))
            );
            assert_eq!(
                run.issues()
                    .iter()
                    .map(|issue| yamaa_core::json::Value::Object(issue.context.clone()))
                    .collect::<Vec<_>>(),
                [
                    yamaa_adapters::json!({"column":"RESULT", "codelist":"DEFAULT", "failure_count":1, "keys":[{"ID":2,"LBTESTCD":"CREAT"}], "values":["X"]}),
                    yamaa_adapters::json!({"column":"RESULT", "codelist":"EXT", "failure_count":1, "keys":[{"ID":1,"LBTESTCD":"GLUC"}], "values":["D"]}),
                ]
            );
            assert!(run.save().is_err());
            // An external dictionary is not enforced, and it still replaces the
            // column's list on its own rows.
            let study = Study::new();
            fs::write(
                study.0.join("spec.yaml"),
                ROW_CT.edit("codelist: GLUCOSE", "codelist: DICT"),
            )
            .unwrap();
            fs::write(study.0.join("project.yaml"), CT_PROJECT).unwrap();
            fs::write(study.0.join("input.csv"), "ID,VALUE\n1,D\n2,\n").unwrap();
            let path = study.0.join("spec.yaml");
            let path = path.to_str().unwrap();
            let project = study.0.join("project.yaml");
            let mut run = project_domain(path, project.to_str().unwrap());
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            assert!(run.save().unwrap());
            assert_eq!(
                fs::read_to_string(study.0.join("output.csv")).unwrap(),
                "ID,RESULT,LBTESTCD\n1,D,GLUC\n2,,GLUC\n1,D,CREAT\n2,,CREAT\n"
            );
        }
        #[test]
        fn per_value_codelist_unknown_and_type_failures_retain_the_authored_row_path() {
            for (codelist, expected) in [
                ("UNKNOWN", "unknown_codelist"),
                ("INT", "codelist_type_mismatch"),
            ] {
                let study = Study::new();
                fs::write(
                    study.0.join("spec.yaml"),
                    ROW_CT.edit("codelist: GLUCOSE", &format!("codelist: {codelist}")),
                )
                .unwrap();
                fs::write(study.0.join("project.yaml"), format!("{CT_PROJECT}      - {{id: INT, name: Numeric values, data_type: integer, items: [{{value: 1}}]}}\n")).unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let project = study.0.join("project.yaml");
                let host = yamaa_core::project_function::Language::Python;
                let Err(error) =
                    yamaa_adapters::file_workflow::select(path, project.to_str().unwrap(), host)
                else {
                    panic!("invalid value codelist was admitted");
                };
                let issues = yamaa_adapters::project_application::selection_failure(&error);
                assert_eq!(issues.len(), 1, "{issues:?}");
                assert_eq!(issues[0].condition(), expected);
                assert_eq!(
                    issues[0].spec_paths,
                    ["columns.RESULT.submission.value_level.GLUC.codelist"]
                );
                assert!(!study.0.join("input.csv").exists());
            }
        }
        /// SUB-12: a where value that the where column's codelists do not
        /// admit fails validation at that where column, before any data is
        /// read. They are its own codelist and those its entries bind,
        /// extended items included; an external dictionary on any of its
        /// rows, or no codelist of its own, admits every value, and a check
        /// without a project refuses the binding (CT-13).
        #[test]
        fn where_values_outside_the_where_columns_codelists_fail_before_data() {
            const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: LB\nkeys: [ID, LBTESTCD]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, LBCAT, LBTESTCD, RESULT]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: LBCAT, type: str, derivation: SRC.CAT}\n  - {name: LBTESTCD, type: str, derivation: SRC.CODE, submission: {origins: [{type: Assigned, source: Sponsor}]<binding>}}\n  - {name: RESULT, type: str, derivation: SRC.VALUE, submission: {origins: [{type: Assigned, source: Sponsor}], value_level: [{id: GLUC, where: {LBTESTCD: [GLUC, <code>]}, length: 4}]}}\n";
            const PROJECT: &str = "yamaa: \"1.0\"\ntype: project\ncodelists:\n  - standard: {name: CDISC/NCI, publishing_set: SDTM, version: '2023-12-15'}\n    codelists:\n      - {id: TESTCD, name: Test codes, extensible: true, items: [{value: GLUC}, {value: CREAT}, {value: SPONSOR, extended: true}]}\n  - codelists:\n      - {id: URINE, name: Urine test codes, items: [{value: UGLUC}]}\n      - {id: DICT, name: Dictionary, external: {dictionary: LOINC, version: '2.77'}}\n";
            const OWN: &str = ", codelist: TESTCD";
            const URINE: &str =
                ", value_level: [{id: URINE, where: {LBCAT: [URINALYSIS]}, codelist: URINE}]";
            const DICTIONARY: &str =
                ", value_level: [{id: URINE, where: {LBCAT: [URINALYSIS]}, codelist: DICT}]";
            let host = yamaa_core::project_function::Language::Python;
            for (binding, code, refused) in [
                (OWN.to_owned(), "GLUCOSE", true),
                // A sponsor addition is one of the extensible list's items.
                (OWN.to_owned(), "SPONSOR", false),
                // An entry of the where column adds its own list's items.
                (format!("{OWN}{URINE}"), "UGLUC", false),
                (format!("{OWN}{URINE}"), "GLUCOSE", true),
                // An external dictionary on any of its rows admits every value,
                (format!("{OWN}{DICTIONARY}"), "GLUCOSE", false),
                (", codelist: DICT".to_owned(), "GLUCOSE", false),
                // as a where column without a codelist of its own does.
                (String::new(), "GLUCOSE", false),
                (URINE.to_owned(), "GLUCOSE", false),
            ] {
                let study = Study::new();
                let spec = SPEC.edit("<binding>", &binding).edit("<code>", code);
                fs::write(study.0.join("spec.yaml"), spec).unwrap();
                fs::write(study.0.join("project.yaml"), PROJECT).unwrap();
                let path = study.0.join("spec.yaml");
                let project = study.0.join("project.yaml");
                let selected = yamaa_adapters::file_workflow::select(
                    path.to_str().unwrap(),
                    project.to_str().unwrap(),
                    host,
                );
                match (selected, refused) {
                    (Ok(_), false) => {}
                    (Ok(_), true) => panic!("{binding} {code}: admitted"),
                    (Err(error), refused) => {
                        let issues = yamaa_adapters::project_application::selection_failure(&error);
                        assert!(refused, "{binding} {code}: {issues:?}");
                        assert_issues(
                            &issues,
                            &[
                                yamaa_adapters::json!({"phase":"validation","condition":"value_level_where_invalid","rule":"SUB-12","spec_paths":["columns.RESULT.submission.value_level.GLUC.where.LBTESTCD"],"context":{"column":"RESULT","entry":"GLUC","reason":"not_in_codelist","values":[code],"where_column":"LBTESTCD"}}),
                            ],
                            &format!("{binding} {code}"),
                        );
                    }
                }
                assert!(!study.0.join("input.csv").exists());
            }
            let study = Study::new();
            let issues = study.check(&SPEC.edit("<binding>", OWN).edit("<code>", "GLUCOSE"));
            assert_eq!(
                issues
                    .iter()
                    .map(|issue| (
                        issue.condition(),
                        issue.rule(),
                        issue.spec_paths.clone(),
                        yamaa_core::json::Value::Object(issue.context.clone())
                    ))
                    .collect::<Vec<_>>(),
                [(
                    "project_binding_missing",
                    Some("CT-13"),
                    vec!["columns.LBTESTCD.submission.codelist".to_owned()],
                    yamaa_adapters::json!({"binding": "codelist", "codelist": "TESTCD", "column": "LBTESTCD"})
                )]
            );
        }
        /// A float codelist's items compare with the column's values exactly
        /// (VALUE-19): 9007199254740992.0 equals no item, although it is the
        /// float nearest the item 9007199254740993, while 0 equals the item
        /// -0.0 and 1.0 the item 1. Missing values pass (CT-7).
        #[test]
        fn float_codelist_items_compare_exactly_and_missing_values_pass() {
            const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input.csv, types: {ID: int, CODE: float}}}\noutput: {path: output.csv, columns: [ID, CODE]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: CODE, type: float, derivation: SRC.CODE, submission: {origins: [{type: Collected, source: Investigator}], codelist: CODES}}\n";
            const PROJECT: &str = "yamaa: \"1.0\"\ntype: project\ncodelists:\n  - codelists:\n      - {id: CODES, name: Codes, data_type: float, items: [{value: 9007199254740993}, {value: -0.0}, {value: 1}]}\n";
            let study = Study::new();
            fs::write(study.0.join("spec.yaml"), SPEC).unwrap();
            fs::write(study.0.join("project.yaml"), PROJECT).unwrap();
            let path = study.0.join("spec.yaml");
            let path = path.to_str().unwrap();
            let project = study.0.join("project.yaml");
            let project = project.to_str().unwrap();
            fs::write(
                study.0.join("input.csv"),
                "ID,CODE\n1,9007199254740992\n2,0\n3,1\n4,\n",
            )
            .unwrap();
            let mut run = project_domain(path, project);
            let issues = run.issues();
            assert_issues(
                &issues,
                &[
                    yamaa_adapters::json!({"phase":"verification","condition":"allowed_values_failed","rule":"CT-7","spec_paths":["columns.CODE.submission.codelist"],"context":{"column":"CODE","codelist":"CODES","failure_count":1,"keys":[{"ID":1}],"values":[9007199254740992.0]}}),
                ],
                "a code outside its codelist",
            );
            assert!(run.save().is_err());
            fs::write(study.0.join("input.csv"), "ID,CODE\n2,0\n3,1\n4,\n").unwrap();
            let mut run = project_domain(path, project);
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            assert!(run.save().unwrap());
            assert_eq!(
                fs::read_to_string(study.0.join("output.csv")).unwrap(),
                "ID,CODE\n2,0\n3,1\n4,\n"
            );
        }

        /// Build one specification on the project route; its run calls no
        /// project function, so the host-functions port is never entered.
        fn project_domain(path: &str, project_path: &str) -> file_application::Domain {
            let project = yamaa_adapters::file_workflow::select(
                path,
                project_path,
                yamaa_core::project_function::Language::Python,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "{:?}",
                    yamaa_adapters::project_application::selection_failure(&error)
                )
            });
            let mut run_scope = project.run_scope(None);
            let attempt = project.build(&mut run_scope, &mut NoFunctions, Scope::default());
            let result = yamaa_adapters::specification_report::build_result(
                project.run(),
                &attempt,
                identity(path),
                &[],
            )
            .unwrap();
            file_application::Domain::new(result, &project, &run_scope)
        }

        #[test]
        fn wide_length_projection_borrows_the_exact_bound_from_compiled_metadata() {
            use yamaa_core::dataset::{CheckRecord, RowIdentity};
            use yamaa_core::{
                dataset::Check, diagnostic::ConditionCode, project_diagnostics::Fact, value::Value,
            };
            let study = Study::new();
            let bound = "340282366920938463463374607431768211456";
            fs::write(
                study.0.join("spec.yaml"),
                VALID.edit("length: 3", &format!("length: {bound}")),
            )
            .unwrap();
            let resources = yamaa_adapters::file_resources::Resources::new(
                study.0.to_str().unwrap(),
                study.0.to_str().unwrap(),
                &[],
            )
            .unwrap();
            let prepared = yamaa_adapters::file_preparation::FileSpecification::prepare(
                resources,
                "spec.yaml",
            )
            .unwrap();
            let compiled = prepared.run().compiled();
            let path = "columns.VALUE.verifications.max_length";
            let Some(Check::WideMaxLength(maximum)) = compiled.column_check(path) else {
                panic!("retained exact length bound");
            };
            assert_eq!(maximum.decimal(), bound);
            // This control tests the formatter's retained observation boundary without
            // pretending a physical native string can exceed this configured bound.
            let records = [CheckRecord {
                path: path.into(),
                code: ConditionCode::VerificationMaxLengthFailed,
                evaluated_count: 1,
                failed_count: 1,
                output_rows: 1,
                offending_rows: vec![RowIdentity {
                    position: 0,
                    values: vec![Value::Int(1)],
                }],
                codelist: None,
                inventory: None,
                row_count: None,
                entries: None,
            }];
            let projected = yamaa_core::verification_diagnostics::project(
                compiled,
                &records,
                yamaa_core::verification_diagnostics::Phase::Verification,
                "spec.yaml",
            )
            .unwrap();
            assert_eq!(
                projected.diagnostics[0].context["max"],
                Fact::Bound(maximum)
            );
            assert!(!study.0.join("input.csv").exists());
        }

        fn inventory_spec() -> String {
            VALID.edit("length: 3", "inventory_vocabulary: true, length: 10")
        }
        const INVENTORY_PROJECT: &str = "yamaa: \"1.0\"\ntype: project\nstudy: {id: STUDY, name: Study, description: Study dataset inventory, protocol_name: Protocol}\nsdtm:\n  standards: [{name: SDTMIG, version: '3.4', status: Final}]\n  specs: [catalog/renamed.yaml]\n  define: define.xml\n";
        /// A run admits a section that lists only its specs: the study and the
        /// section's standard and define are required only by Define.
        const SPECS_ONLY_PROJECT: &str =
            "yamaa: \"1.0\"\ntype: project\nsdtm:\n  specs: [catalog/renamed.yaml]\n";
        /// A project that lists no dataset holds an empty inventory, which
        /// admits only a missing value (CT-11). A 1.x specification binds its
        /// inventory vocabulary only with a project file (CT-13).
        const NO_DATASETS_PROJECT: &str = "yamaa: \"1.0\"\ntype: project\n";
        #[test]
        fn absent_study_inventory_checks_nonmissing_values_and_keeps_missing_values_admitted() {
            for input in ["ID,VALUE\n1,DM\n2,\n", "ID,VALUE\n1,\n"] {
                let study = Study::new();
                fs::write(study.0.join("spec.yaml"), inventory_spec()).unwrap();
                fs::write(study.0.join("project.yaml"), NO_DATASETS_PROJECT).unwrap();
                fs::write(study.0.join("input.csv"), input).unwrap();
                let path = study.0.join("spec.yaml");
                let project = study.0.join("project.yaml");
                let run = project_domain(path.to_str().unwrap(), project.to_str().unwrap());
                if input.contains("DM") {
                    assert_eq!(run.issues().len(), 1);
                    assert_eq!(
                        (
                            run.issues()[0].phase(),
                            run.issues()[0].condition(),
                            run.issues()[0].rule()
                        ),
                        ("verification", "allowed_values_failed", Some("CT-11"))
                    );
                    assert_eq!(
                        run.issues()[0].spec_paths,
                        ["columns.VALUE.submission.inventory_vocabulary"]
                    );
                    assert_eq!(yamaa_adapters::json_wire::encode_object(&run.issues()[0].context),"{\"column\":\"VALUE\",\"failure_count\":1,\"keys\":[{\"ID\":1}],\"values\":[\"DM\"]}");
                } else {
                    assert!(run.issues().is_empty(), "{:?}", run.issues());
                }
            }
        }
        #[test]
        fn inventory_uses_inherited_declared_dataset_names_and_retained_original_metadata_in_both_hosts(
        ) {
            use yamaa_core::project_function::Language;
            for (text, language) in [
                (INVENTORY_PROJECT, Language::Python),
                (INVENTORY_PROJECT, Language::R),
                (SPECS_ONLY_PROJECT, Language::Python),
                (SPECS_ONLY_PROJECT, Language::R),
            ] {
                let study = Study::new();
                fs::create_dir(study.0.join("catalog")).unwrap();
                fs::write(study.0.join("spec.yaml"), inventory_spec()).unwrap();
                fs::write(study.0.join("project.yaml"), text).unwrap();
                fs::write(
                    study.0.join("catalog/renamed.yaml"),
                    "yamaa: \"1.0\"\ntype: spec\nparent: parent.yaml\n",
                )
                .unwrap();
                fs::write(
                    study.0.join("catalog/parent.yaml"),
                    VALID
                        .edit("domain: TEST", "domain: DM")
                        .edit("input.csv", "never-read.csv"),
                )
                .unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let project_path = study.0.join("project.yaml");
                let project = yamaa_adapters::file_workflow::select(
                    path,
                    project_path.to_str().unwrap(),
                    language,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{:?}",
                        yamaa_adapters::project_application::selection_failure(&error)
                    )
                });
                assert_eq!(project.run().environment().inventory().names(), ["DM"]);
                let retained = project.run().captured_environment().inventory_documents();
                assert_eq!(retained.len(), 1);
                assert!(retained[0]
                    .source()
                    .identity
                    .ends_with("catalog/renamed.yaml"));
                assert!(!study.0.join("catalog/never-read.csv").exists());
                assert!(!study.0.join("input.csv").exists());
                fs::write(study.0.join("input.csv"), "ID,VALUE\n1,DM\n2,renamed\n3,\n").unwrap();
                let mut run_scope = project.run_scope(None);
                let attempt = project.build(&mut run_scope, &mut NoFunctions, Scope::default());
                let result = yamaa_adapters::specification_report::build_result(
                    project.run(),
                    &attempt,
                    identity(path),
                    &[],
                )
                .unwrap();
                let run = file_application::Domain::new(result, &project, &run_scope);
                assert_eq!(run.issues().len(), 1);
                assert_eq!(run.issues()[0].rule(), Some("CT-11"));
                assert_eq!(yamaa_adapters::json_wire::encode_object(&run.issues()[0].context),"{\"column\":\"VALUE\",\"failure_count\":1,\"keys\":[{\"ID\":2}],\"values\":[\"renamed\"]}");
                assert!(!study.0.join("define.xml").exists());
            }
        }
        #[test]
        fn value_inventory_override_inherits_base_metadata_and_selects_only_its_test_code() {
            let study = Study::new();
            let raw = ROW_CT
                .edit("codelist: DEFAULT", "inventory_vocabulary: true")
                .edit("codelist: GLUCOSE", "inventory_vocabulary: false");
            fs::write(study.0.join("spec.yaml"), raw).unwrap();
            fs::write(study.0.join("project.yaml"), NO_DATASETS_PROJECT).unwrap();
            fs::write(study.0.join("input.csv"), "ID,VALUE\n1,DM\n2,\n").unwrap();
            let path = study.0.join("spec.yaml");
            let project = study.0.join("project.yaml");
            let run = project_domain(path.to_str().unwrap(), project.to_str().unwrap());
            assert_eq!(run.issues().len(), 1);
            assert_eq!(yamaa_adapters::json_wire::encode_object(&run.issues()[0].context),"{\"column\":\"RESULT\",\"failure_count\":1,\"keys\":[{\"ID\":1,\"LBTESTCD\":\"CREAT\"}],\"values\":[\"DM\"]}");
        }

        #[test]
        fn value_codelist_checks_report_one_row_per_declaring_path_in_success_and_failure() {
            for fail in [false, true] {
                let study = Study::new();
                fs::write(
                    study.0.join("spec.yaml"),
                    ROW_CT.edit("path: output.csv", "path: output.csv, report: checks.csv"),
                )
                .unwrap();
                let project = if fail {
                    CT_PROJECT.to_owned()
                } else {
                    CT_PROJECT.edit("items: [{value: G}]", "items: [{value: D}]")
                };
                fs::write(study.0.join("project.yaml"), project).unwrap();
                fs::write(study.0.join("input.csv"), "ID,VALUE\n1,D\n2,\n").unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let project = study.0.join("project.yaml");
                let mut run = project_domain(path, project.to_str().unwrap());
                assert_eq!(run.issues().len(), usize::from(fail));
                // The run, its input, both row templates, the value-level
                // selection check, the entry's warning that it matches a row
                // (SUB-13) and one check row per bound codelist; a successful
                // run adds its three columns.
                assert_eq!(run.report_table().rows(), if fail { 8 } else { 11 });
                assert!(!study.0.join("checks.csv").exists());
                let saved = run.save();
                assert_eq!(saved.is_ok(), !fail);
                let checks = fs::read_to_string(study.0.join("checks.csv")).unwrap();
                for path in [
                    ",check,columns.RESULT.submission.codelist,allowed_values,,RESULT,CT-7,error,",
                    ",check,columns.RESULT.submission.value_level.GLUC.codelist,allowed_values,,RESULT,CT-7,error,",
                    ",check,columns.RESULT.submission.value_level,value_level,,RESULT,SUB-13,error,held,,0,4,,{}",
                    ",check,columns.RESULT.submission.value_level.GLUC,value_level,,RESULT,SUB-13,warning,held,,0,1,,{}",
                ] {
                    assert_eq!(checks.matches(path).count(), 1, "{checks}");
                }
                assert_eq!(checks.matches("CT-7").count(), 2);
                assert_eq!(study.0.join("output.csv").exists(), !fail);
            }
        }

        fn value_length_spec(length: &str) -> String {
            ROW_CT
                .edit("codelist: DEFAULT", "length: 3")
                .edit("codelist: GLUCOSE", &format!("length: {length}"))
        }
        #[test]
        fn per_value_lengths_replace_inherited_bounds_only_for_their_completed_test_code() {
            for (bound, input, expected_path, expected_key, maximum) in [
                (
                    "5",
                    "ID,VALUE\n1,abcde\n2,é字😀\n",
                    "columns.RESULT.verifications.max_length",
                    "CREAT",
                    3,
                ),
                (
                    "2",
                    "ID,VALUE\n1,abc\n2,é字\n3,\n",
                    "columns.RESULT.submission.value_level.GLUC.length",
                    "GLUC",
                    2,
                ),
            ] {
                let study = Study::new();
                fs::write(study.0.join("spec.yaml"), value_length_spec(bound)).unwrap();
                fs::write(study.0.join("input.csv"), input).unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let mut run = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(path),
                )
                .unwrap();
                assert_eq!(run.issues().len(), 1, "{:?}", run.issues());
                let issue = &run.issues()[0];
                assert_eq!(
                    (issue.phase(), issue.condition(), issue.rule()),
                    ("verification", "length_failed", Some("CHECK-8"))
                );
                assert_eq!(issue.spec_paths, [expected_path]);
                assert_eq!(
                    yamaa_core::json::Value::Object(issue.context.clone()),
                    yamaa_adapters::json!({"column":"RESULT","failure_count":1,"keys":[{"ID":1,"LBTESTCD":expected_key}],"max":maximum})
                );
                assert!(run.save().is_err());
            }
        }
        #[test]
        fn equal_per_value_lengths_run_one_shared_check_and_wide_bounds_keep_exact_metadata() {
            for length in ["3", "340282366920938463463374607431768211456"] {
                let study = Study::new();
                fs::write(
                    study.0.join("spec.yaml"),
                    value_length_spec(length)
                        .edit("path: output.csv", "path: output.csv, report: checks.csv"),
                )
                .unwrap();
                fs::write(study.0.join("input.csv"), "ID,VALUE\n1,é字😀\n2,\n").unwrap();
                let path = study.0.join("spec.yaml");
                let path = path.to_str().unwrap();
                let mut run = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: path,
                        environment: None,
                    },
                    identity(path),
                )
                .unwrap();
                assert!(run.issues().is_empty(), "{:?}", run.issues());
                // The run, its input, two row templates and three columns,
                // then the two value-level checks and one shared length check
                // or one check per bound.
                assert_eq!(
                    run.report_table().rows(),
                    if length == "3" { 10 } else { 11 }
                );
                assert!(run.save().unwrap());
                assert_eq!(
                    fs::read_to_string(study.0.join("output.csv")).unwrap(),
                    "ID,RESULT,LBTESTCD\n1,é字😀,GLUC\n2,,GLUC\n1,é字😀,CREAT\n2,,CREAT\n"
                );
            }
        }
        /// A where clause compares text, so each where column must be a declared
        /// `str` column (SUB-12). An entry over an integer test code fails
        /// before any data is read, whatever metadata it carries.
        #[test]
        fn entries_over_a_nontext_where_column_fail_before_data() {
            let integer = |raw: String| {
                raw.edit("{name: LBTESTCD, type: str}", "{name: LBTESTCD, type: int}")
                    .edit("\"'GLUC'\"", "\"'1'\"")
                    .edit("\"'CREAT'\"", "\"'2'\"")
            };
            let refused = |issues: &[yamaa_core::diagnostic::Diagnostic]| {
                assert_issues(
                    issues,
                    &[
                        yamaa_adapters::json!({"phase":"validation","condition":"value_level_where_invalid","rule":"SUB-12","spec_paths":["columns.RESULT.submission.value_level.GLUC.where.LBTESTCD"],"context":{"column":"RESULT","entry":"GLUC","reason":"not_str","where_column":"LBTESTCD"}}),
                    ],
                    "a where value that is not text",
                );
            };
            let study = Study::new();
            let origin_only = ROW_CT.edit("codelist: DEFAULT", "length: 3").edit(
                "codelist: GLUCOSE",
                "origins: [{type: Collected, source: Vendor}]",
            );
            for raw in [value_length_spec("5"), origin_only] {
                refused(&study.check(&integer(raw)));
            }
            // A per-value codelist the project binds is refused the same way.
            fs::write(study.0.join("spec.yaml"), integer(ROW_CT.into())).unwrap();
            fs::write(study.0.join("project.yaml"), CT_PROJECT).unwrap();
            let path = study.0.join("spec.yaml");
            let project = study.0.join("project.yaml");
            let Err(error) = yamaa_adapters::file_workflow::select(
                path.to_str().unwrap(),
                project.to_str().unwrap(),
                yamaa_core::project_function::Language::Python,
            ) else {
                panic!("a per-value codelist over an integer where column was admitted");
            };
            refused(&yamaa_adapters::project_application::selection_failure(
                &error,
            ));
            assert!(!study.0.join("input.csv").exists());
        }
        #[test]
        fn effective_value_metadata_rejects_invalid_bounds_types_and_explicit_length_conflicts_before_data(
        ) {
            for (declaration, condition, path) in [
                (
                    "length: -999999999999999999999999999999",
                    "submission_length_invalid",
                    "columns.RESULT.submission.value_level.GLUC.length",
                ),
                (
                    "length: 0",
                    "submission_length_invalid",
                    "columns.RESULT.submission.value_level.GLUC.length",
                ),
                (
                    "data_type: float",
                    "submission_data_type_not_admitted",
                    "columns.RESULT.submission.value_level.GLUC",
                ),
                (
                    "significant_digits: -999999999999999999999999999999",
                    "submission_length_invalid",
                    "columns.RESULT.submission.value_level.GLUC.significant_digits",
                ),
            ] {
                let study = Study::new();
                let raw = ROW_CT
                    .edit("codelist: DEFAULT", "length: 3")
                    .edit("codelist: GLUCOSE", declaration);
                let issues = study.check(&raw);
                assert_eq!(issues.len(), 1, "{declaration}: {issues:?}");
                assert_eq!(issues[0].condition(), condition);
                assert_eq!(issues[0].spec_paths, [path]);
                assert!(!study.0.join("input.csv").exists());
            }
            let study = Study::new();
            let raw = value_length_spec("4").edit(
                "derivation: SRC.VALUE, submission",
                "derivation: SRC.VALUE, verifications: [{max_length: {max: 3}}], submission",
            );
            let issues = study.check(&raw);
            assert_issues(
                &issues,
                &[
                    yamaa_adapters::json!({"phase":"validation","condition":"declared_length_conflict","rule":"SUB-8","spec_paths":["columns.RESULT.submission.value_level.GLUC"],"context":{"column":"RESULT","length":4,"max_length":3}}),
                ],
                "an entry length above the column's max_length",
            );
        }

        fn domain(study: &Study, raw: &str, input: &str) -> file_application::Domain {
            fs::write(study.0.join("spec.yaml"), raw).unwrap();
            fs::write(study.0.join("input.csv"), input).unwrap();
            let path = study.0.join("spec.yaml");
            let path = path.to_str().unwrap();
            crate::support::study::domain(
                yamaa_core::project_function::Language::Python,
                Request {
                    specification: path,
                    environment: None,
                },
                identity(path),
            )
            .unwrap()
        }
        /// The SPEC_PATH, SEVERITY and CONDITION of each violated check row of a
        /// run's report, in report order.
        fn violated_checks(
            report: &yamaa_adapters::public_table::PublicTable,
        ) -> Vec<(String, String, String)> {
            use arrow_array::{cast::AsArray, Array};
            let cell = |array: &dyn Array, row: usize| {
                if array.is_null(row) {
                    String::new()
                } else if let Some(strings) = array.as_string_opt::<i32>() {
                    strings.value(row).to_owned()
                } else {
                    array.as_string_view().value(row).to_owned()
                }
            };
            let mut violated = Vec::new();
            for batch in report.batches() {
                let column = |name: &str| batch.column_by_name(name).unwrap().as_ref();
                for row in 0..batch.num_rows() {
                    if cell(column("OUTCOME"), row) == "violated" {
                        violated.push((
                            cell(column("SPEC_PATH"), row),
                            cell(column("SEVERITY"), row),
                            cell(column("CONDITION"), row),
                        ));
                    }
                }
            }
            violated
        }
        const FINDINGS: &str = "submission: {label: Results, class: FINDINGS, structure: One record per test, repeating: true}\n";
        #[test]
        fn mandatory_columns_are_checked_without_a_study_document() {
            let missing = "ID,VALUE\n1,abc\n2,\n";
            for (raw, path) in [
                (
                    VALID.edit(
                        "submission: {length: 3",
                        "submission: {mandatory: true, length: 3",
                    ),
                    "columns.VALUE.submission.mandatory",
                ),
                (
                    format!(
                        "{}{FINDINGS}",
                        VALID.edit(
                            "submission: {length: 3",
                            "submission: {core: Req, length: 3"
                        )
                    ),
                    "columns.VALUE.submission.core",
                ),
            ] {
                let study = Study::new();
                let mut run = domain(&study, &raw, missing);
                assert_eq!(run.issues().len(), 1, "{path}: {:?}", run.issues());
                let issue = &run.issues()[0];
                assert_eq!(
                    (issue.phase(), issue.condition(), issue.rule()),
                    ("verification", "not_missing_failed", Some("SUB-11"))
                );
                assert_eq!(issue.spec_paths, [path]);
                assert_eq!(
                    yamaa_adapters::json_wire::encode_object(&issue.context),
                    "{\"column\":\"VALUE\",\"failure_count\":1,\"keys\":[{\"ID\":2}]}"
                );
                assert!(run.save().is_err());
                assert!(!study.0.join("output.csv").exists());
            }
            // `core` derives nothing without a declared class, nor for an
            // analysis class, whose ADaM columns declare `mandatory` (SUB-10),
            // so the same missing value is admitted.
            let adam = VALID
                .edit("model: SDTM", "model: ADaM")
                .edit("{type: Assigned, source: Sponsor}", "{type: Assigned}");
            for raw in [
                VALID.edit(
                    "submission: {length: 3",
                    "submission: {core: Req, length: 3",
                ),
                format!(
                    "{}{}",
                    adam.edit(
                        "submission: {length: 3",
                        "submission: {core: Req, mandatory: false, length: 3"
                    ),
                    FINDINGS.edit("FINDINGS", "BASIC DATA STRUCTURE")
                ),
            ] {
                let study = Study::new();
                let mut run = domain(&study, &raw, missing);
                assert!(run.issues().is_empty(), "{raw}: {:?}", run.issues());
                assert!(run.save().unwrap());
            }
        }
        #[test]
        fn value_level_mandatory_applies_only_to_the_rows_of_its_test_code() {
            let missing = "ID,VALUE\n1,abc\n2,\n";
            for (base, value, path, code) in [
                (
                    "length: 3",
                    "mandatory: true",
                    "columns.RESULT.submission.value_level.GLUC.mandatory",
                    "GLUC",
                ),
                (
                    "mandatory: true, length: 3",
                    "mandatory: false",
                    "columns.RESULT.submission.mandatory",
                    "CREAT",
                ),
            ] {
                let raw = ROW_CT
                    .edit("codelist: DEFAULT", base)
                    .edit("codelist: GLUCOSE", value);
                let study = Study::new();
                let mut run = domain(&study, &raw, missing);
                assert_eq!(run.issues().len(), 1, "{path}: {:?}", run.issues());
                let issue = &run.issues()[0];
                assert_eq!(
                    (issue.condition(), issue.rule()),
                    ("not_missing_failed", Some("SUB-11"))
                );
                assert_eq!(issue.spec_paths, [path]);
                assert_eq!(
                    yamaa_core::json::Value::Object(issue.context.clone()),
                    yamaa_adapters::json!({"column":"RESULT","failure_count":1,"keys":[{"ID":2,"LBTESTCD":code}]})
                );
                assert!(run.save().is_err());
                let study = Study::new();
                let mut run = domain(&study, &raw, "ID,VALUE\n1,abc\n2,xyz\n");
                assert!(run.issues().is_empty(), "{path}: {:?}", run.issues());
                assert!(run.save().unwrap());
            }
        }
        /// Entries over two where columns: one by test code, one by specimen;
        /// nothing static rules out a glucose urine row.
        const SELECTED: &str = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: LB\nkeys: [ID, LBTESTCD]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, LBTESTCD, LBSPEC, RESULT], report: report.csv}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: LBTESTCD, type: str}\n  - {name: LBSPEC, type: str, derivation: SRC.SPEC}\n  - name: RESULT\n    type: str\n    derivation: SRC.VALUE\n    submission:\n      origins: [{type: Assigned, source: Sponsor}]\n      value_level:\n        - {id: GLUC, where: {LBTESTCD: [GLUC]}, mandatory: true}\n        - {id: URINE, where: {LBSPEC: [URINE]}, length: 2}\n        - {id: ALB, where: {LBTESTCD: [ALB], LBSPEC: [SERUM]}}\nrows:\n  - id: glucose\n    derivations: {LBTESTCD: \"'GLUC'\"}\n  - id: creatinine\n    derivations: {LBTESTCD: \"'CREAT'\"}\n";
        #[test]
        fn a_row_two_entries_select_fails_with_its_keys_and_both_entries() {
            let study = Study::new();
            assert!(study.check(SELECTED).is_empty());
            let mut run = domain(&study, SELECTED, "ID,SPEC,VALUE\n1,SERUM,5\n2,URINE,7\n");
            assert_eq!(run.issues().len(), 1, "{:?}", run.issues());
            let issue = &run.issues()[0];
            assert_eq!(
                (issue.phase(), issue.condition(), issue.rule()),
                (
                    "verification",
                    "value_level_multiple_matches",
                    Some("SUB-13")
                )
            );
            assert_eq!(issue.spec_paths, ["columns.RESULT.submission.value_level"]);
            assert_eq!(
                yamaa_core::json::Value::Object(issue.context.clone()),
                yamaa_adapters::json!({"column": "RESULT", "entries": [["GLUC", "URINE"]], "failure_count": 1, "keys": [{"ID": 2, "LBTESTCD": "GLUC"}]})
            );
            assert!(run.save().is_err());
            assert!(!study.0.join("output.csv").exists());
        }
        #[test]
        fn an_entry_that_matches_no_row_is_a_warning_and_scoped_checks_read_their_rows() {
            let study = Study::new();
            // Only serum is collected: URINE and ALB select no row, which warns
            // without failing. The warning needs no report file: every run's
            // report carries it, and it is never an issue.
            let quiet = SELECTED.edit(", report: report.csv", "");
            assert!(study.check(&quiet).is_empty());
            let input = "ID,SPEC,VALUE\n1,SERUM,5\n2,SERUM,toolong\n";
            let mut run = domain(&study, &quiet, input);
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            assert_eq!(
                violated_checks(run.report_table()),
                [
                    (
                        "columns.RESULT.submission.value_level.URINE".to_owned(),
                        "warning".to_owned(),
                        "value_level_unmatched".to_owned()
                    ),
                    (
                        "columns.RESULT.submission.value_level.ALB".to_owned(),
                        "warning".to_owned(),
                        "value_level_unmatched".to_owned()
                    ),
                ]
            );
            assert!(run.save().unwrap());
            assert!(!study.0.join("report.csv").exists());
            let study = Study::new();
            let mut run = domain(&study, SELECTED, input);
            assert!(run.issues().is_empty(), "{:?}", run.issues());
            assert!(run.save().unwrap());
            let report = fs::read_to_string(study.0.join("report.csv")).unwrap();
            for row in [
                "output.csv,check,columns.RESULT.submission.value_level,value_level,,RESULT,SUB-13,error,held,,0,4,,{}",
                "output.csv,check,columns.RESULT.submission.value_level.GLUC,value_level,,RESULT,SUB-13,warning,held,,0,1,,{}",
                "output.csv,check,columns.RESULT.submission.value_level.URINE,value_level,,RESULT,SUB-13,warning,violated,value_level_unmatched,1,1,[{}],\"{\"\"column\"\":\"\"RESULT\"\",\"\"entry\"\":\"\"URINE\"\"}\"",
                "output.csv,check,columns.RESULT.submission.value_level.ALB,value_level,,RESULT,SUB-13,warning,violated,value_level_unmatched,1,1,[{}],\"{\"\"column\"\":\"\"RESULT\"\",\"\"entry\"\":\"\"ALB\"\"}\"",
                "output.csv,check,columns.RESULT.submission.value_level.GLUC.mandatory,not_missing,,RESULT,SUB-11,error,held,,0,2,,{}",
                "output.csv,check,columns.RESULT.submission.value_level.URINE.length,max_length,,RESULT,CHECK-8,error,held,,0,0,,{}",
            ] {
                assert_eq!(report.matches(row).count(), 1, "{report}");
            }
            // The glucose entry's `mandatory` decides the glucose rows alone.
            let study = Study::new();
            let mut run = domain(&study, SELECTED, "ID,SPEC,VALUE\n1,SERUM,\n");
            assert_eq!(run.issues().len(), 1, "{:?}", run.issues());
            let issue = &run.issues()[0];
            assert_eq!(issue.condition(), "not_missing_failed");
            assert_eq!(
                issue.spec_paths,
                ["columns.RESULT.submission.value_level.GLUC.mandatory"]
            );
            assert_eq!(
                yamaa_core::json::Value::Object(issue.context.clone()),
                yamaa_adapters::json!({"column": "RESULT", "failure_count": 1, "keys": [{"ID": 1, "LBTESTCD": "GLUC"}]})
            );
            assert!(run.save().is_err());
        }
        #[test]
        fn where_clauses_name_declared_str_output_columns_and_cannot_overlap() {
            let study = Study::new();
            for (from, to, condition, path) in [
                (
                    "where: {LBSPEC: [URINE]}",
                    "where: {LBORNAM: [URINE]}",
                    "value_level_where_invalid",
                    vec!["columns.RESULT.submission.value_level.URINE.where.LBORNAM"],
                ),
                (
                    "where: {LBSPEC: [URINE]}",
                    "where: {ID: [URINE]}",
                    "value_level_where_invalid",
                    vec!["columns.RESULT.submission.value_level.URINE.where.ID"],
                ),
                (
                    "where: {LBSPEC: [URINE]}",
                    "where: {LBSPEC: []}",
                    "value_level_where_invalid",
                    vec!["columns.RESULT.submission.value_level.URINE.where.LBSPEC"],
                ),
                (
                    "{id: URINE, where: {LBSPEC: [URINE]}",
                    "{id: GLUC, where: {LBSPEC: [URINE]}",
                    "duplicate_identifier",
                    vec!["columns.RESULT.submission.value_level.GLUC.id"],
                ),
                (
                    "where: {LBTESTCD: [ALB], LBSPEC: [SERUM]}",
                    "where: {LBTESTCD: [GLUC], LBSPEC: [SERUM]}",
                    "value_level_overlap",
                    vec![
                        "columns.RESULT.submission.value_level.GLUC.where",
                        "columns.RESULT.submission.value_level.ALB.where",
                    ],
                ),
            ] {
                let raw = SELECTED.replace(from, to);
                assert_ne!(raw, SELECTED);
                let issues = study.check(&raw);
                assert_eq!(issues.len(), 1, "{to}: {issues:?}");
                assert_eq!(
                    (issues[0].phase(), issues[0].condition()),
                    ("validation", condition)
                );
                assert_eq!(issues[0].spec_paths, path);
            }
            assert!(!study.0.join("input.csv").exists());
        }
        #[test]
        fn restated_facts_and_relaxed_keys_fail_validation_before_data() {
            let key_check = VALID.edit(
                "{name: ID, type: int, derivation: SRC.ID}",
                "{name: ID, type: int, derivation: SRC.ID, verifications: [not_missing]}",
            );
            let mandatory_check = VALID.edit(
                "    submission: {length: 3",
                "    verifications: [not_missing]\n    submission: {mandatory: true, length: 3",
            );
            let key_unique = format!("{VALID}verifications: [{{unique: [ID]}}]\n");
            let relaxed_key = VALID.edit(
                "{name: ID, type: int, derivation: SRC.ID}",
                "{name: ID, type: int, derivation: SRC.ID, submission: {mandatory: false, length: 8, origins: [{type: Assigned, source: Sponsor}]}}",
            );
            for (raw, condition, rule, path, context) in [
                (
                    key_check,
                    "redundant_declaration",
                    "CHECK-5",
                    "columns.ID.verifications[0].not_missing",
                    "{\"column\":\"ID\",\"stated_by\":\"keys[0]\"}",
                ),
                (
                    mandatory_check,
                    "redundant_declaration",
                    "SUB-11",
                    "columns.VALUE.verifications[0].not_missing",
                    "{\"column\":\"VALUE\",\"stated_by\":\"columns.VALUE.submission.mandatory\"}",
                ),
                (
                    key_unique,
                    "redundant_declaration",
                    "CHECK-10",
                    "verifications[0].unique",
                    "{\"columns\":[\"ID\"],\"stated_by\":\"keys\"}",
                ),
                (
                    relaxed_key,
                    "value_not_permitted",
                    "SUB-10",
                    "columns.ID.submission.mandatory",
                    "{\"column\":\"ID\"}",
                ),
            ] {
                let study = Study::new();
                let issues = study.check(&raw);
                assert_issues(
                    &issues,
                    &[
                        yamaa_adapters::json!({"phase":"validation","condition":condition,"rule":rule,"spec_paths":[path],"context":yamaa_adapters::json_wire::decode(context).unwrap()}),
                    ],
                    path,
                );
                assert!(!study.0.join("input.csv").exists());
            }
            // A `not_missing` restates nothing when an entry relaxes the column's
            // `mandatory`, and a `unique` over more than the keys is its own claim.
            let study = Study::new();
            let relaxed = ROW_CT
                .edit(
                    "submission: {origins: [{type: Assigned, source: Sponsor}], codelist: DEFAULT,",
                    "verifications: [not_missing], submission: {origins: [{type: Assigned, source: Sponsor}], mandatory: true,",
                )
                .edit("codelist: GLUCOSE", "mandatory: false");
            assert!(study.check(&relaxed).is_empty());
            let wider = format!("{VALID}verifications: [{{unique: [ID, VALUE]}}]\n");
            assert!(study.check(&wider).is_empty());
        }
        #[test]
        fn effective_value_fields_inherit_full_metadata_and_refute_inapplicable_lengths_and_digits()
        {
            let root = "\nsubmission: {label: Laboratory Results, class: FINDINGS, structure: One record per test, repeating: true}\n";
            let base = format!(
                "{}{root}",
                value_length_spec("3").edit(
                    "origins: [{type: Assigned, source: Sponsor}], length: 3",
                    "core: Perm, origins: [{type: Assigned, source: Sponsor}], length: 3"
                )
            );
            let study = Study::new();
            let baseline = study.check(&base);
            assert!(baseline.is_empty(), "{baseline:?}");
            for (entry, field) in [
                ("data_type: date", "data_type"),
                ("significant_digits: 2", "field"),
            ] {
                let raw = base.edit(
                    "where: {LBTESTCD: [GLUC]}, length: 3}",
                    &format!("where: {{LBTESTCD: [GLUC]}}, {entry}}}"),
                );
                let issues = study.check(&raw);
                assert_eq!(issues.len(), 1, "{entry}: {issues:?}");
                assert_eq!(
                    (issues[0].condition(), issues[0].rule()),
                    ("submission_length_not_applicable", Some("SUB-7"))
                );
                assert_eq!(
                    issues[0].spec_paths,
                    ["columns.RESULT.submission.value_level.GLUC"]
                );
                let context = issues[0].context.clone();
                assert_eq!(context["column"], "RESULT");
                assert_eq!(
                    context[field],
                    if field == "field" {
                        "significant_digits"
                    } else {
                        "date"
                    }
                );
            }
            assert!(!study.0.join("input.csv").exists());
        }

        /// SUB-19's recode, literal and stored-value rows, as a SEND study
        /// declares them, since SEND keeps the Not Available origin that SDTM
        /// refuses (SUB-15): each origin below is admitted by its derivation.
        const ORIGINS: &str = r#"yamaa: "1.0"
type: spec
model: SEND
domain: DM
keys: [ID]
input: {SRC: {path: input.csv, types: {ID: int, AGE: int, EGFR: float, WEIGHT: float, HEIGHT: float}}}
output: {path: output.csv, columns: [ID, SEX, RACE, BRTHDTC, RFICDT, HEIGHTU, EGFR, DTHDTC, AGE, BMI, AGEGR1, ARMCD]}
columns:
  - {name: ID, type: int, derivation: SRC.ID}
  - name: SEX
    type: str
    derivation: {mapping: {source: SRC.SEX, dict: {Male: M, Female: F}}}
    submission: {origins: [{type: Collected, source: Investigator}]}
  - name: RACE
    type: str
    derivation:
      case:
        - {when: "SRC.RACE = 'white'", then: "'WHITE'"}
        - {otherwise: {str_case: {source: SRC.RACE, to: upper}}}
    submission: {origins: [{type: Collected, source: Investigator}]}
  - name: BRTHDTC
    type: str
    derivation: {str_template: "{SRC.BRTHYR}-{SRC.BRTHMO}-{SRC.BRTHDY}"}
    submission: {origins: [{type: Collected, source: Subject}]}
  - name: RFICDT
    type: date
    derivation: {to_date: {source: SRC.RFICDTC}}
    submission: {origins: [{type: Collected, source: Investigator}]}
  - name: HEIGHTU
    type: str
    derivation: "'cm'"
    submission: {origins: [{type: Collected, source: Investigator}]}
  - name: EGFR
    type: float
    derivation: SRC.EGFR
    submission:
      origins: [{type: Derived, source: Vendor}]
      method: Estimated by the central laboratory from serum creatinine.
  - name: DTHDTC
    type: str
    derivation: null
    submission: {origins: [{type: Not Available, description: Death date was not collected.}]}
  - {name: AGE, type: int, derivation: SRC.AGE}
  - {name: WEIGHT, type: float, derivation: SRC.WEIGHT}
  - {name: HEIGHT, type: float, derivation: SRC.HEIGHT}
  - name: BMI
    type: float
    derivation: "WEIGHT / (HEIGHT * HEIGHT)"
    submission:
      origins: [{type: Derived, source: Sponsor}]
      method: Weight in kilograms divided by the square of height in metres.
  - name: AGEGR1
    type: str
    derivation:
      case:
        - {when: "AGE >= 65", then: "'>=65'"}
        - {otherwise: "'<65'"}
    submission:
      origins: [{type: Derived, source: Sponsor}]
      method: Age grouped at 65 years.
  - name: ARMCD
    type: str
    derivation: {mapping: {source: SRC.ARM, dict: {Placebo: PBO, Active: ACT}}}
    submission: {origins: [{type: Assigned, source: Sponsor}]}
"#;
        #[test]
        fn recodes_literals_and_stored_values_admit_their_published_origins() {
            // A recode of collected terminology, a case over collected values,
            // an ISO 8601 assembly of date parts, a printed unit and a value
            // computed upstream and read as stored keep the origin a study
            // declares for them; Not Available is admitted by every derivation.
            let study = Study::new();
            let issues = study.check(ORIGINS);
            assert!(issues.is_empty(), "{issues:?}");
            for (column, declared) in [
                ("BMI", "Not Available, description: Not derived."),
                ("AGEGR1", "Not Available, description: Not derived."),
                ("HEIGHTU", "Protocol, source: Sponsor"),
                ("EGFR", "Collected, source: Vendor"),
            ] {
                let raw = replace_origin(ORIGINS, column, declared);
                let issues = study.check(&raw);
                assert!(issues.is_empty(), "{column}: {issues:?}");
            }
            assert!(!study.0.join("input.csv").exists());
        }
        /// Replace one column's declared origin in [`ORIGINS`].
        fn replace_origin(raw: &str, column: &str, origin: &str) -> String {
            let start = raw.find(&format!("name: {column}\n")).unwrap();
            let at = start + raw[start..].find("origins: [{type: ").unwrap();
            let end = at + raw[at..].find('}').unwrap();
            format!("{}origins: [{{type: {origin}{}", &raw[..at], &raw[end..])
        }
        #[test]
        fn computed_values_and_case_reads_of_derived_columns_refute_collected() {
            let study = Study::new();
            for (column, declared, admitted) in [
                // Arithmetic computes the value.
                (
                    "BMI",
                    "Collected, source: Vendor",
                    &["Assigned", "Derived", "Not Available", "Other"][..],
                ),
                // A case whose condition reads an output column classifies a
                // derived value rather than recoding a collected one.
                (
                    "AGEGR1",
                    "Collected, source: Investigator",
                    &["Assigned", "Derived", "Not Available", "Other"],
                ),
                // A recode refutes Protocol and a literal refutes Derived.
                (
                    "ARMCD",
                    "Protocol, source: Sponsor",
                    &["Assigned", "Collected", "Derived", "Not Available", "Other"],
                ),
                (
                    "HEIGHTU",
                    "Derived, source: Sponsor",
                    &[
                        "Assigned",
                        "Collected",
                        "Not Available",
                        "Other",
                        "Protocol",
                    ],
                ),
            ] {
                let raw = replace_origin(ORIGINS, column, declared);
                let issues = study.check(&raw);
                let refuted: Vec<_> = issues
                    .iter()
                    .filter(|issue| issue.condition() == "origin_contradicts_derivation")
                    .collect();
                assert_eq!(refuted.len(), 1, "{column}: {issues:?}");
                assert_eq!(refuted[0].rule(), Some("SUB-19"));
                assert_eq!(
                    refuted[0].spec_paths,
                    [format!("columns.{column}.submission")]
                );
                let context = refuted[0].context.clone();
                assert_eq!(context["column"], column);
                assert_eq!(context["origin_type"], declared.split(',').next().unwrap());
                assert_eq!(context["admitted"], yamaa_adapters::json!(admitted));
            }
            // Reading a declared input dataset instead makes AGEGR1 a recode.
            let raw = replace_origin(
                &ORIGINS.edit("when: \"AGE >= 65\"", "when: \"SRC.AGE >= 65\""),
                "AGEGR1",
                "Collected, source: Investigator",
            );
            assert!(study.check(&raw).is_empty());
            assert!(!study.0.join("input.csv").exists());
        }

        /// SUB-19's bare reads beyond a `source`: an `ONLY` pivot of long
        /// collected records and a plain lookup copy a stored value.
        const READS: &str = r#"yamaa: "1.0"
type: spec
model: SDTM
domain: LB
keys: [USUBJID, LBTESTCD]
input: {ODM: input.csv, RANGES: ranges.csv}
intermediates:
  - id: RANGE
    dataset: RANGES
    key: {TESTCD: LBTESTCD}
output: {path: output.csv, columns: [USUBJID, LBTESTCD, LBORRES, LBORNRLO]}
columns:
  - {name: USUBJID, type: str, derivation: ODM.SubjectKey}
  - {name: LBTESTCD, type: str}
  - name: LBORRES
    type: str
    submission: {origins: [{type: Collected, source: Investigator}]}
  - name: LBORNRLO
    type: str
    derivation: RANGE.LOW
    submission: {origins: [{type: Collected, source: Vendor}]}
rows:
  - id: glucose
    dataset: ODM
    group_by: [ODM.SubjectKey]
    derivations:
      LBTESTCD: "'GLUC'"
      LBORRES:
        aggregate:
          filter: "ODM.ItemOID = 'IT.LB.GLUC'"
          expr: "ONLY(ODM.Value)"
"#;
        #[test]
        fn pivots_and_plain_lookups_are_bare_reads_that_admit_collected() {
            // A pivot selects the one collected value of its group, and a lookup
            // that declares no derivations copies the record it selects.
            let study = Study::new();
            let issues = study.check(READS);
            assert!(issues.is_empty(), "{issues:?}");
            let shorthand = READS.edit(
                "aggregate:\n          filter: \"ODM.ItemOID = 'IT.LB.GLUC'\"\n          expr: \"ONLY(ODM.Value)\"",
                "aggregate: \"ONLY(ODM.Value)\"",
            );
            assert!(study.check(&shorthand).is_empty());
            for (from, to, column) in [
                // Any other reduction computes the value it returns.
                ("ONLY(ODM.Value)", "MAX(ODM.Value)", "LBORRES"),
                // A lookup that derives fields is not a plain copy.
                (
                    "key: {TESTCD: LBTESTCD}\n",
                    "key: {TESTCD: LBTESTCD}\n    derivations:\n      LOWTEXT: {str_case: {source: LOW, to: upper}}\n",
                    "LBORNRLO",
                ),
            ] {
                let issues = study.check(&READS.replacen(from, to, 1));
                let refuted: Vec<_> = issues
                    .iter()
                    .filter(|issue| issue.condition() == "origin_contradicts_derivation")
                    .collect();
                assert_eq!(refuted.len(), 1, "{column}: {issues:?}");
                assert_eq!(
                    refuted[0].spec_paths,
                    [format!("columns.{column}.submission")]
                );
                assert_eq!(refuted[0].context["origin_type"], "Collected");
                assert_eq!(
                    refuted[0].context["admitted"],
                    yamaa_adapters::json!(["Assigned", "Derived", "Not Available", "Other"])
                );
            }
            // A plain lookup is a bare read, but it is no predecessor's dataset.
            let issues = study.check(&READS.edit(
                "{origins: [{type: Collected, source: Vendor}]}",
                "{origins: [{type: Predecessor, description: RANGE.LOW}]}",
            ));
            assert_eq!(issues.len(), 1, "{issues:?}");
            assert_eq!(issues[0].condition(), "origin_contradicts_derivation");
            assert_eq!(issues[0].context["reason"], "predecessor_source");
            assert!(!study.0.join("input.csv").exists());
        }

        const PREDECESSORS: &str = r#"yamaa: "1.0"
type: spec
model: ADaM
domain: ADSL
keys: [USUBJID]
input: {DM: dm.csv, SUPPDM: {path: suppdm.csv, key: [USUBJID]}}
base: DM
output: {path: adsl.csv, columns: [USUBJID, AGE, RACEOTH]}
columns:
  - {name: USUBJID, type: str, label: Unique Subject Identifier, derivation: DM.USUBJID}
  - name: AGE
    type: str
    label: Age
    derivation: DM.AGE
    submission: {origins: [{type: Predecessor, description: DM.AGE}]}
  - name: RACEOTH
    type: str
    label: Race, Other
    derivation: {source: {variable: SUPPDM.QVAL, filter: "SUPPDM.QNAM = 'RACEOTH'"}}
    submission: {origins: [{type: Predecessor, description: "SUPPDM.QVAL where QNAM = 'RACEOTH'"}]}
"#;
        #[test]
        fn predecessor_columns_copy_the_column_their_description_names() {
            let study = Study::new();
            let issues = study.check(PREDECESSORS);
            assert!(issues.is_empty(), "{issues:?}");
            for (from, to, column, reason, predecessor) in [
                (
                    "derivation: DM.AGE",
                    "derivation: DM.USUBJID",
                    "AGE",
                    "predecessor_source",
                    "DM.AGE",
                ),
                (
                    "derivation: {source: {variable: SUPPDM.QVAL, filter: \"SUPPDM.QNAM = 'RACEOTH'\"}}",
                    "derivation: SUPPDM.QVAL",
                    "RACEOTH",
                    "predecessor_filter",
                    "SUPPDM.QVAL",
                ),
            ] {
                let issues = study.check(&PREDECESSORS.replace(from, to));
                assert_eq!(issues.len(), 1, "{column}: {issues:?}");
                assert_eq!(
                    (
                        issues[0].condition(),
                        issues[0].rule()
                    ),
                    ("origin_contradicts_derivation", Some("SUB-19"))
                );
                assert_eq!(issues[0].spec_paths, [format!("columns.{column}.submission")]);
                let context = issues[0].context.clone();
                assert_eq!(context["column"], column);
                assert_eq!(context["origin_type"], "Predecessor");
                assert_eq!(context["predecessor"], predecessor);
                assert_eq!(context["reason"], reason);
            }
            // A description that names no `<dataset>.<column>` is not the form.
            let issues = study.check(&PREDECESSORS.edit(
                "description: DM.AGE}",
                "description: Copied from demographics}",
            ));
            assert_eq!(issues.len(), 1, "{issues:?}");
            assert_eq!(
                (issues[0].condition(), issues[0].rule()),
                ("origin_description_missing", Some("SUB-16"))
            );
            assert_eq!(
                issues[0].spec_paths,
                ["columns.AGE.submission.origins[0].description"]
            );
            assert_eq!(issues[0].context["reason"], "predecessor_form");
            // A computed column cannot claim to be a copy.
            let issues = study.check(&PREDECESSORS.edit(
                "derivation: DM.AGE",
                "derivation: {str_template: \"{DM.AGE} years\"}",
            ));
            assert_eq!(issues.len(), 1, "{issues:?}");
            assert_eq!(issues[0].condition(), "origin_contradicts_derivation");
            // A value-level Predecessor is checked against the derivations
            // that can produce the rows its where clause selects.
            let values = "yamaa: \"1.0\"\ntype: spec\nmodel: SDTM\ndomain: LB\nkeys: [ID, LBTESTCD]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID, LBTESTCD, LBORRES]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n  - {name: LBTESTCD, type: str}\n  - {name: LBORRES, type: str, submission: {origins: [{type: Assigned, source: Sponsor}], value_level: [{id: GLUC, where: {LBTESTCD: [GLUC]}, origins: [{type: Predecessor, description: SRC.GLUC}]}]}}\nrows:\n  - id: glucose\n    derivations: {LBTESTCD: \"'GLUC'\", LBORRES: SRC.GLUC}\n  - id: creatinine\n    derivations: {LBTESTCD: \"'CREAT'\", LBORRES: SRC.CREAT}\n";
            assert!(study.check(values).is_empty());
            let issues =
                study.check(&values.replacen("LBORRES: SRC.GLUC", "LBORRES: SRC.CREAT", 1));
            assert_issues(
                &issues,
                &[
                    yamaa_adapters::json!({"phase":"validation","condition":"refuted_value_origin","rule":"SUB-13","spec_paths":["columns.LBORRES.submission.value_level.GLUC"],"context":{"column":"LBORRES","derivation_kind":"source","entry":"GLUC","origin_type":"Predecessor","reason":"predecessor_source"}}),
                ],
                "a predecessor entry whose column reads another source",
            );
            assert!(!study.0.join("dm.csv").exists());
        }
        #[test]
        fn inventory_names_the_datasets_of_every_submission_section() {
            // An ADaM SRCDOM names the SDTM domain another section lists.
            let study = Study::new();
            fs::create_dir(study.0.join("sdtm")).unwrap();
            fs::write(
                study.0.join("spec.yaml"),
                // ADaM derives an Assigned origin's source (SUB-15).
                inventory_spec()
                    .edit("domain: TEST", "domain: ADAE")
                    .edit("model: SDTM", "model: ADaM")
                    .edit("{type: Assigned, source: Sponsor}", "{type: Assigned}"),
            )
            .unwrap();
            fs::write(
                study.0.join("sdtm/ae.yaml"),
                VALID
                    .edit("domain: TEST", "domain: AE")
                    .edit("input.csv", "never-read.csv"),
            )
            .unwrap();
            fs::write(
                study.0.join("environment.yaml"),
                "yamaa: \"1.0\"\ntype: project\nstudy: {id: STUDY, name: Study, description: Study dataset inventory, protocol_name: Protocol}\nsdtm:\n  standards: [{name: SDTMIG, version: '3.4', status: Final}]\n  specs: [sdtm/ae.yaml]\n  define: sdtm/define.xml\nadam:\n  standards: [{name: ADaMIG, version: '1.3', status: Final}]\n  specs: [spec.yaml]\n  define: define.xml\n",
            )
            .unwrap();
            fs::write(
                study.0.join("input.csv"),
                "ID,VALUE\n1,AE\n2,ADAE\n3,XX\n4,\n",
            )
            .unwrap();
            let path = study.0.join("spec.yaml");
            let environment = study.0.join("environment.yaml");
            let run = project_domain(path.to_str().unwrap(), environment.to_str().unwrap());
            assert_eq!(run.issues().len(), 1, "{:?}", run.issues());
            assert_eq!(
                (
                    run.issues()[0].phase(),
                    run.issues()[0].condition(),
                    run.issues()[0].rule()
                ),
                ("verification", "allowed_values_failed", Some("CT-11"))
            );
            assert_eq!(
                yamaa_adapters::json_wire::encode_object(&run.issues()[0].context),
                "{\"column\":\"VALUE\",\"failure_count\":1,\"keys\":[{\"ID\":3}],\"values\":[\"XX\"]}"
            );
            assert!(!study.0.join("sdtm/never-read.csv").exists());
            assert!(!study.0.join("define.xml").exists());
        }
    }
}
