#![cfg(feature = "yaml")]
//! File contracts: producer links.
mod file {
    mod producer_links {
        //! A `schema` link types a stored artifact from its producing
        //! specification's output contract. The producer is never executed:
        //! callers build and save it first, and the consumer reads the file at
        //! its own `path` like any other input.
        use crate::support::Edit as _;
        use std::fs;
        use yamaa_adapters::{
            file_application::{self, Domain},
            specification_report::Identity,
        };
        use yamaa_core::diagnostic::Diagnostic;
        use yamaa_core::domain_entry::Request;

        struct Study(crate::support::Scratch);
        impl Study {
            fn new() -> Self {
                let root = crate::support::Scratch::canonical("producer-links");
                Self(root)
            }
            fn path(&self, written: &str) -> String {
                let path = self.0.join(written).to_str().unwrap().to_owned();
                #[cfg(windows)]
                let path = path.replace('\\', "/");
                path
            }
            fn write(&self, written: &str, bytes: impl AsRef<[u8]>) {
                let path = self.0.join(written);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, bytes).unwrap();
            }
            fn read(&self, written: &str) -> Vec<u8> {
                fs::read(self.0.join(written)).unwrap()
            }
            fn build(&self, entry: &str) -> Domain {
                let path = self.path(entry);
                crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    identity(&path),
                )
                .unwrap_or_else(|issues| panic!("{entry}: {issues:?}"))
            }
            fn check(&self, entry: &str) -> Vec<Diagnostic> {
                let path = self.path(entry);
                file_application::check(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    identity(&path),
                )
            }
        }
        fn identity(specification: &str) -> Identity<'_> {
            crate::support::study::identity("producer-links", specification)
        }
        /// Seven authored records of one subject, counted into one record.
        const RAW: &str = "ID,VALUE\n1,1\n1,2\n1,3\n1,4\n1,5\n1,6\n1,7\n";
        fn producer(profile: &str, value: &str) -> String {
            format!("yamaa: \"1.0\"\ntype: spec\ndomain: COUNTED\nkeys: [ID]\ninput: {{RAW: {{path: ../raw/raw.csv, types: {{ID: int, VALUE: int}}}}}}\nrows:\n  - id: counted\n    dataset: RAW\n    group_by: [RAW.ID]\n    derivations: {{ID: RAW.ID, VALUE: {value}}}\ncolumns:\n  - {{name: ID, type: int, label: Identifier}}\n  - {{name: VALUE, type: int, label: Count}}\noutput: {{path: ../generated/producer.{profile}, columns: [ID, VALUE]}}\n")
        }
        fn consumer(profile: &str, extra: &str) -> String {
            format!("yamaa: \"1.0\"\ntype: spec\ndomain: ROOT\nkeys: [ID]\ninput: {{FIRST: {{path: ../generated/producer.{profile}, schema: ../producer/p.yaml{extra}}}}}\nrows:\n  - id: counted\n    dataset: FIRST\n    group_by: [FIRST.ID, FIRST.VALUE]\n    derivations: {{ID: FIRST.ID, SOURCE_COUNT: FIRST.VALUE, VALUE: {{aggregate: 'COUNT(FIRST.*)'}}}}\ncolumns:\n  - {{name: ID, type: int, label: Identifier}}\n  - {{name: SOURCE_COUNT, type: int, label: Original source count}}\n  - {{name: VALUE, type: int, label: Count}}\noutput: {{path: ../generated/root.csv, columns: [ID, SOURCE_COUNT, VALUE]}}\n")
        }
        fn profiles() -> Vec<&'static str> {
            vec![
                "csv",
                #[cfg(feature = "parquet")]
                "parquet",
            ]
        }
        fn study(profile: &str) -> Study {
            let study = Study::new();
            study.write("raw/raw.csv", RAW);
            study.write(
                "producer/p.yaml",
                producer(profile, "{aggregate: 'COUNT(RAW.*)'}"),
            );
            study.write("consumer/root.yaml", consumer(profile, ""));
            study
        }

        #[test]
        fn callers_build_and_save_the_producer_before_its_consumer_reads_the_artifact() {
            for profile in profiles() {
                let study = study(profile);
                // Static checking reads the producing specification, not data.
                assert!(study.check("consumer/root.yaml").is_empty(), "{profile}");
                // Before the producer is built and saved, the consumer's input is
                // an ordinary missing file (`producer_links.json`), and the build
                // has no output.
                let early = study.build("consumer/root.yaml");
                assert!(early.output().is_none());
                assert!(!study.0.join("generated").exists());

                fs::create_dir(study.0.join("generated")).unwrap();
                let mut produced = study.build("producer/p.yaml");
                assert!(produced.issues().is_empty(), "{:?}", produced.issues());
                assert!(produced.save().unwrap());
                if profile == "csv" {
                    assert_eq!(study.read("generated/producer.csv"), b"ID,VALUE\n1,7\n");
                }
                let mut consumed = study.build("consumer/root.yaml");
                assert!(consumed.issues().is_empty(), "{:?}", consumed.issues());
                assert!(consumed.save().unwrap());
                assert_eq!(
                    study.read("generated/root.csv"),
                    b"ID,SOURCE_COUNT,VALUE\n1,7,1\n"
                );

                // The producer never runs inside the consumer: its own source can
                // disappear and the consumer still reads the stored artifact.
                fs::remove_file(study.0.join("raw/raw.csv")).unwrap();
                fs::remove_file(study.0.join("generated/root.csv")).unwrap();
                let mut again = study.build("consumer/root.yaml");
                assert!(again.issues().is_empty(), "{:?}", again.issues());
                assert!(again.save().unwrap());
                assert_eq!(
                    study.read("generated/root.csv"),
                    b"ID,SOURCE_COUNT,VALUE\n1,7,1\n"
                );
            }
        }

        #[test]
        fn a_consumer_may_not_write_a_file_of_the_producer_it_reads() {
            let study = study("csv");
            study.write("generated/producer.csv", "ID,VALUE\n1,7\n");
            for output in [
                "path: ../generated/producer.csv",
                "path: ../generated/root.csv, report: ../generated/producer.csv",
            ] {
                study.write(
                    "consumer/root.yaml",
                    consumer("csv", "").edit("path: ../generated/root.csv", output),
                );
                // Refused while it is prepared, before any study read, with
                // the collision `producer_links.json` records.
                let path = study.path("consumer/root.yaml");
                let refused = crate::support::study::domain(
                    yamaa_core::project_function::Language::Python,
                    Request {
                        specification: &path,
                        environment: None,
                    },
                    identity(&path),
                );
                assert!(refused.is_err(), "the collision refuses the build");
                assert_eq!(study.read("generated/producer.csv"), b"ID,VALUE\n1,7\n");
            }
        }

        #[test]
        fn the_producing_specification_is_read_but_never_compiled_or_executed() {
            let study = Study::new();
            // The producer's derivation calls an unknown function and its source
            // does not exist; only its output contract is read.
            study.write(
                "producer/p.yaml",
                producer("csv", "{function: {name: missing, args: {x: RAW.VALUE}}}"),
            );
            study.write("consumer/root.yaml", consumer("csv", ""));
            study.write("generated/producer.csv", "ID,VALUE\n4,2\n4,2\n5,3\n");
            let mut consumed = study.build("consumer/root.yaml");
            assert!(consumed.issues().is_empty(), "{:?}", consumed.issues());
            assert!(consumed.save().unwrap());
            assert_eq!(
                study.read("generated/root.csv"),
                b"ID,SOURCE_COUNT,VALUE\n4,2,2\n5,3,1\n"
            );
            assert!(!study.0.join("raw").exists());
        }

        /// Each stored header's mismatch is a case of `producer_links.json`;
        /// a build that reports it has no output.
        #[test]
        fn a_stored_header_must_equal_the_producer_output_columns_in_order() {
            for stored in ["VALUE,ID\n7,1\n", "ID\n1\n", "ID,VALUE,OTHER\n1,7,x\n"] {
                let study = study("csv");
                study.write("generated/producer.csv", stored);
                let result = study.build("consumer/root.yaml");
                assert!(result.output().is_none(), "{stored}");
            }
        }

        #[cfg(feature = "parquet")]
        #[test]
        fn a_stored_parquet_schema_must_equal_the_producer_output_types() {
            let study = study("parquet");
            // Another specification stores VALUE as text at the linked path.
            study.write(
                "producer/text.yaml",
                producer("parquet", "\"'7'\"").edit(
                    "{name: VALUE, type: int, label: Count}",
                    "{name: VALUE, type: str, label: Count}",
                ),
            );
            fs::create_dir(study.0.join("generated")).unwrap();
            let mut text = study.build("producer/text.yaml");
            assert!(text.issues().is_empty(), "{:?}", text.issues());
            assert!(text.save().unwrap());
            // The mismatch is `producer_links.json`'s; the build has no output.
            let result = study.build("consumer/root.yaml");
            assert!(result.output().is_none());
        }

        #[test]
        fn a_linked_ordinal_is_local_to_the_consumer_and_never_part_of_the_contract() {
            let study = study("csv");
            study.write("generated/producer.csv", "ID,VALUE\n1,7\n2,7\n");
            study.write(
                "consumer/root.yaml",
                "yamaa: \"1.0\"\ntype: spec\ndomain: ROOT\nkeys: [ID]\nbase: FIRST\ninput: {FIRST: {path: ../generated/producer.csv, schema: ../producer/p.yaml, ordinal: REC}}\ncolumns:\n  - {name: ID, type: int, label: Identifier, derivation: FIRST.ID}\n  - {name: POSITION, type: int, label: Position, derivation: FIRST.REC}\noutput: {path: ../generated/root.csv, columns: [ID, POSITION]}\n",
            );
            let mut result = study.build("consumer/root.yaml");
            assert!(result.issues().is_empty(), "{:?}", result.issues());
            assert!(result.save().unwrap());
            assert_eq!(study.read("generated/root.csv"), b"ID,POSITION\n1,1\n2,2\n");

            // An ordinal naming a contract field collides with it
            // (`producer_links.json`); the build has no output.
            study.write("consumer/root.yaml", consumer("csv", ", ordinal: ID"));
            let result = study.build("consumer/root.yaml");
            assert!(result.output().is_none());
        }
    }
}
