//! Encoder contracts: the Define-XML document and the transport files the
//! adapters write over a composed study, and the refusals only an encoder
//! raises (DEFINE-24, DEFINE-25, XPT-10). Each test calls an encoder over a
//! study composed in memory, the seam its first comment names; the define
//! route's cases (the application suite's
//! `fixtures/diagnostics/shared_define.json`) reach the
//! encoders only through a project's whole composition from files (#3778).
mod submission_encoders {
    /// Studies composed from independently authored trees, as core's
    /// composition tests compose them.
    mod fixture {
        pub(crate) use crate::support::document::Tree;
        pub(crate) use crate::support::roots as rt;
        use std::convert::Infallible;
        use yamaa_core::{
            project_terminology as ct,
            submission_composition::{self as composition, Finding, Input, Study},
            table::{CellError, Column, TableAccess, TableSchema, ValueRef},
            value::{ColumnType, Value},
        };

        pub(crate) fn metadata() -> Tree {
            Tree::map(vec![
                ("core", Tree::text("Exp")),
                ("length", Tree::integer("8")),
                (
                    "origins",
                    Tree::list(vec![Tree::map(vec![
                        ("type", Tree::text("Assigned")),
                        ("source", Tree::text("Sponsor")),
                    ])]),
                ),
            ])
        }
        pub(crate) fn column(name: &str) -> Tree {
            Tree::map(vec![
                ("name", Tree::text(name)),
                ("type", Tree::text("str")),
                ("label", Tree::text("Independent column")),
                ("derivation", Tree::text("'X'")),
                ("submission", metadata()),
            ])
        }
        /// A 1.0 specification (D4) of the `SDTM` model (D25), which the
        /// `sdtm` section lists.
        pub(crate) fn specification() -> Tree {
            Tree::map(vec![
                ("yamaa", Tree::text("1.0")),
                ("type", Tree::text("spec")),
                ("model", Tree::text("SDTM")),
                ("domain", Tree::text("D0")),
                ("keys", Tree::list(vec![Tree::text("VALUE")])),
                (
                    "submission",
                    Tree::map(vec![
                        ("label", Tree::text("Independent dataset")),
                        ("class", Tree::text("SPECIAL PURPOSE")),
                        ("structure", Tree::text("One record per subject")),
                        ("repeating", Tree::boolean(false)),
                    ]),
                ),
                ("columns", Tree::list(vec![column("VALUE")])),
                (
                    "output",
                    Tree::map(vec![
                        ("path", Tree::text("d0.csv")),
                        ("columns", Tree::list(vec![Tree::text("VALUE")])),
                    ]),
                ),
            ])
        }
        /// A 1.0 project (D4) whose `sdtm` section lists `d0.yaml` under its
        /// `standards` (D16).
        pub(crate) fn project() -> Tree {
            Tree::map(vec![
                ("yamaa", Tree::text("1.0")),
                ("type", Tree::text("project")),
                (
                    "study",
                    Tree::map(vec![
                        ("id", Tree::text("S01")),
                        ("name", Tree::text("Independent study")),
                        ("description", Tree::text("Submission detector witness")),
                        ("protocol_name", Tree::text("P01")),
                    ]),
                ),
                (
                    "sdtm",
                    Tree::map(vec![
                        (
                            "standards",
                            Tree::list(vec![Tree::map(vec![
                                ("name", Tree::text("SDTMIG")),
                                ("version", Tree::text("3.4")),
                                ("status", Tree::text("Final")),
                            ])]),
                        ),
                        ("context", Tree::text("Other")),
                        ("define", Tree::text("define.xml")),
                        ("specs", Tree::list(vec![Tree::text("d0.yaml")])),
                    ]),
                ),
            ])
        }
        /// Compose the project's first section over `docs`.
        pub(crate) fn compose(
            project: Tree,
            docs: Vec<Tree>,
            catalogue: &ct::Catalogue,
            created: &str,
        ) -> Result<Study, Vec<Finding>> {
            let project =
                yamaa_core::front_end::v1::project::project(&project.document(), None).unwrap();
            let declared = project.submission.unwrap();
            let docs: Vec<_> = docs.into_iter().map(Tree::document).collect();
            let trees: Vec<_> = docs
                .iter()
                .map(|document| yamaa_core::front_end::v1::spec::lower(document, None).into_spec())
                .collect();
            let inputs: Vec<_> = docs
                .iter()
                .zip(&trees)
                .map(|(document, tree)| Input {
                    document,
                    tree,
                    source: "d0.yaml",
                    artifact: "d0.csv",
                    href: Some("d0.csv"),
                })
                .collect();
            let section = &declared.sections[0];
            let required = composition::Required::admit(declared.study.as_ref(), section)?;
            composition::compose(required, catalogue, &inputs, &[], "1.0", created)
        }
        pub(crate) fn one(
            project: Tree,
            spec: Tree,
            catalogue: &ct::Catalogue,
        ) -> Result<Study, Vec<Finding>> {
            compose(project, vec![spec], catalogue, "2026-09-30T00:00:00")
        }

        /// Held rows of one typed column; a declared row count may exceed the
        /// cells when only the file geometry is inspected.
        pub(crate) struct Rows {
            pub(crate) schema: TableSchema,
            pub(crate) rows: usize,
            pub(crate) values: Vec<Value>,
        }
        impl TableAccess for Rows {
            type Error = Infallible;
            fn schema(&self) -> &TableSchema {
                &self.schema
            }
            fn row_count(&self) -> usize {
                self.rows
            }
            fn cell(
                &self,
                row: usize,
                column: usize,
            ) -> Result<ValueRef<'_>, CellError<Infallible>> {
                match self.values.get(row) {
                    Some(value) if column == 0 => Ok(value.into()),
                    _ => Err(CellError::OutOfBounds { row, column }),
                }
            }
        }
        pub(crate) fn rows(kind: ColumnType, values: Vec<Value>) -> Rows {
            Rows {
                schema: TableSchema::new(vec![Column {
                    name: "VALUE".into(),
                    kind,
                }])
                .unwrap(),
                rows: values.len(),
                values,
            }
        }
    }

    mod define_xml {
        use super::fixture::*;
        use yamaa_adapters::submission_xml::render;
        use yamaa_core::{
            diagnostic::ConditionCode, json::Value as JsonValue, project_terminology as ct,
            submission_composition::Finding, value::Value,
        };

        fn detail(finding: &Finding) -> &JsonValue {
            &finding.context["detail"]
        }

        /// Seam: the Define-XML encoder's own finding over a study composed
        /// in memory; the define route's issue for such a label is the
        /// `public-define` host fixture's `column-label` case. DEFINE-24: a
        /// tab in a column label is refused where the label is written,
        /// naming the label's spec path.
        #[test]
        fn a_tab_in_generated_text_is_untransportable_at_the_field_that_holds_it() {
            let mut spec = specification();
            *spec.at(&["columns", "0", "label"]) = Tree::text("A\tB");
            let empty = ct::Catalogue::admit(vec![]).unwrap();
            let study = one(project(), spec, &empty).unwrap();
            let finding = render(&study).unwrap_err();
            assert_eq!(finding.code, ConditionCode::SubmissionXmlWhitespace);
            assert_eq!(finding.definition().rule, Some("DEFINE-24"), "{finding:?}");
            assert_eq!(finding.path, "datasets.D0.columns.VALUE.label");
            assert_eq!(
                detail(&finding),
                &JsonValue::String("forbidden XML whitespace".into())
            );
            // The label writes a tab, which the finding names.
            assert_eq!(
                finding.context["whitespace"],
                JsonValue::String("tab".into())
            );
            assert_eq!(finding.context.len(), 2);
        }

        /// Seam: the encoder's schema check over a composed study whose
        /// creation time the define route refuses before it composes
        /// (`shared_define.json::a_creation_date_without_its_time_is_refused`),
        /// so no case reaches it. DEFINE-25: a creation time outside the
        /// schema's whole-second civil datetime is refused before any byte is
        /// written.
        #[test]
        fn a_creation_time_outside_the_schema_facet_is_a_schema_violation() {
            let empty = ct::Catalogue::admit(vec![]).unwrap();
            let study = compose(
                project(),
                vec![specification()],
                &empty,
                "invalid civil timestamp",
            )
            .unwrap();
            let finding = render(&study).unwrap_err();
            assert_eq!(finding.code, ConditionCode::SubmissionXmlInvalid);
            assert_eq!(finding.definition().rule, Some("DEFINE-25"));
            assert_eq!(finding.path, "created");
            assert_eq!(
                detail(&finding),
                &JsonValue::String("schema_violation: CreationDateTime".into())
            );
        }

        /// Seam: the encoder over a study composed in memory against a
        /// catalogue admitted from in-memory sources, which a project file's
        /// composition does not build. DEFINE-9 and DEFINE-10: sources that
        /// cite one terminology standard
        /// share its one `def:Standard`, which each codelist bound from them
        /// references, and a source the document binds nothing from is not
        /// written.
        #[test]
        fn sources_citing_one_terminology_standard_share_its_one_definition() {
            const NCI: &str = "CDISC/NCI";
            let source = |name: &str, version: &str, list: &str| {
                rt::Source {
                    standard: Some(rt::Standard {
                        name: name.into(),
                        publishing_set: "SDTM".into(),
                        version: version.into(),
                    }),
                    codelists: vec![rt::Codelist {
                        id: list.into(),
                        name: format!("Independent list {list}"),
                        data_type: rt::CodelistType::Text,
                        extensible: false,
                        alias: None,
                        format_name: None,
                        external: None,
                        items: Some(vec![rt::Item {
                            value: Value::Str("X".into()),
                            decode: None,
                            rank: None,
                            alias: None,
                            extended: false,
                        }]),
                    }],
                }
                .root()
            };
            let bound = |name: &str, list: &str| {
                let mut column = column(name);
                column.set(&["submission", "codelist"], Tree::text(list));
                column
            };
            let mut spec = specification();
            spec.set(
                &["columns"],
                Tree::list(vec![bound("VALUE", "FIRST"), bound("OTHER", "SECOND")]),
            );
            spec.set(
                &["output", "columns"],
                Tree::list(vec![Tree::text("VALUE"), Tree::text("OTHER")]),
            );
            let (cited, later) = ("2026-03-27", "2026-06-26");
            let study = one(
                project(),
                spec,
                &ct::Catalogue::admit(vec![
                    source(NCI, cited, "FIRST"),
                    source(NCI, later, "UNBOUND"),
                    source(NCI, cited, "SECOND"),
                ])
                .unwrap(),
            )
            .unwrap();
            let xml = String::from_utf8(render(&study).unwrap()).unwrap();
            assert_eq!(
                xml.matches("<def:Standard OID=\"STD.CT_SDTM\"").count(),
                1,
                "{xml}"
            );
            assert_eq!(
                xml.matches("def:StandardOID=\"STD.CT_SDTM\"").count(),
                2,
                "{xml}"
            );
            assert!(!xml.contains(later), "{xml}");
        }
    }

    /// Value-level metadata as the document writes it (DEFINE-27): one
    /// value list per column, its ordered item and where-clause references,
    /// and identifiers that survive reordering. Each test renders a study
    /// composed in memory from a document tree (`submission_composition`),
    /// not a project's composition from files, which is the define route's.
    mod value_level {
        use yamaa_core::{
            front_end::v1,
            project_terminology::Catalogue,
            schema::{Document, DocumentNode as N},
            submission_composition::{self, Input, Study},
        };

        #[derive(Default)]
        struct Arena(Vec<N>);
        impl Arena {
            fn node(&mut self, node: N) -> usize {
                let n = self.0.len();
                self.0.push(node);
                n
            }
            fn text(&mut self, text: &str) -> usize {
                self.node(N::Text(text.into()))
            }
            fn map(&mut self, fields: Vec<(&str, usize)>) -> usize {
                let fields = fields
                    .into_iter()
                    .map(|(key, value)| (self.text(key), value))
                    .collect();
                self.node(N::Mapping(fields))
            }
            fn texts(&mut self, fields: &[(&str, &str)]) -> usize {
                let fields = fields
                    .iter()
                    .map(|&(key, value)| (key, self.text(value)))
                    .collect();
                self.map(fields)
            }
            fn strings(&mut self, values: &[&str]) -> usize {
                let values = values.iter().map(|value| self.text(value)).collect();
                self.node(N::Sequence(values))
            }
            fn finish(self, root: usize) -> Document {
                Document::new(self.0, root).unwrap()
            }
        }
        fn project() -> Document {
            let mut a = Arena::default();
            let yamaa = a.text("1.0");
            let kind = a.text("project");
            let study = a.texts(&[
                ("id", "VLSTUDY"),
                ("name", "Value metadata study"),
                ("description", "Independent structure witness"),
                ("protocol_name", "VLPROTOCOL"),
            ]);
            let standard = a.texts(&[("name", "SDTMIG"), ("version", "3.4"), ("status", "Final")]);
            let standards = a.node(N::Sequence(vec![standard]));
            let filename = a.text("spec.yaml");
            let specs = a.node(N::Sequence(vec![filename]));
            let define = a.text("define.xml");
            let version = a.text("1.0");
            let section = a.map(vec![
                ("standards", standards),
                ("specs", specs),
                ("define", define),
                ("source_system_version", version),
            ]);
            let root = a.map(vec![
                ("yamaa", yamaa),
                ("type", kind),
                ("study", study),
                ("sdtm", section),
            ]);
            a.finish(root)
        }
        /// One entry of `LBORRESU`, in the order the case lists them: its id,
        /// its where clause and its origin source.
        struct Written<'a> {
            id: &'a str,
            clause: Vec<(&'a str, Vec<&'a str>)>,
            source: &'a str,
        }
        fn written(case: &str) -> Vec<Written<'_>> {
            let entry = |id, column, values: &[&'static str], source| Written {
                id,
                clause: vec![(column, values.to_vec())],
                source,
            };
            let gluc = entry("GLUC", "LBTESTCD", &["GLUC"], "Vendor");
            let creat = entry("CREAT", "LBTESTCD", &["CREAT"], "Sponsor");
            match case {
                "reordered values" => vec![creat, gluc],
                "inserted value" => {
                    vec![entry("ALB", "LBTESTCD", &["ALB"], "Sponsor"), gluc, creat]
                }
                "membership" => vec![entry("CHEM", "LBTESTCD", &["GLUC", "CREAT"], "Vendor")],
                "conjunction" => vec![
                    Written {
                        id: "GLUCSER",
                        clause: vec![("LBTESTCD", vec!["GLUC"]), ("LBSPEC", vec!["SERUM"])],
                        source: "Vendor",
                    },
                    creat,
                ],
                "escaped value" => vec![
                    entry("GLUC", "LBTESTCD", &["A.&\" /\u{e9}"], "Vendor"),
                    creat,
                ],
                _ => vec![gluc, creat],
            }
        }
        fn specification(case: &str) -> Document {
            let mut a = Arena::default();
            let yamaa = a.text("1.0");
            let kind = a.text("spec");
            let model = a.text("SDTM");
            let domain = a.text("LB");
            let label = a.text("Laboratory tests");
            let class = a.text("FINDINGS");
            let structure = a.text("One record per test");
            let repeating = a.node(N::Boolean(true));
            let dataset = a.map(vec![
                ("label", label),
                ("class", class),
                ("structure", structure),
                ("repeating", repeating),
            ]);
            let mut columns = Vec::new();
            for (name, kind) in [
                ("LBTESTCD", "str"),
                ("LBSPEC", "str"),
                ("LBNAM", "str"),
                ("LBSEQ", "int"),
                ("LBORRESU", "str"),
            ] {
                let name_node = a.text(name);
                let kind = a.text(kind);
                let label = a.text(match name {
                    "LBORRESU" => "Original units",
                    "LBSPEC" => "Specimen type",
                    "LBNAM" => "Laboratory name",
                    "LBSEQ" => "Sequence number",
                    _ => "Test code",
                });
                let mut fields = vec![("name", name_node), ("type", kind), ("label", label)];
                let literal = match name {
                    "LBORRESU" => Some(a.text("'mg/dL'")),
                    "LBSPEC" => Some(a.text("'SERUM'")),
                    "LBNAM" => Some(a.text("'CENTRAL'")),
                    "LBSEQ" => Some(a.node(N::Integer("1".into()))),
                    _ => None,
                };
                if let Some(literal) = literal {
                    let derivation = a.map(vec![("value", literal)]);
                    fields.push(("derivation", derivation));
                }
                // The internal column carries no submission metadata.
                if name == "LBNAM" {
                    columns.push(a.map(fields));
                    continue;
                }
                let origin = a.texts(&[("type", "Assigned"), ("source", "Sponsor")]);
                let origins = a.node(N::Sequence(vec![origin]));
                let core = a.text("Exp");
                let length = a.node(N::Integer("8".into()));
                let mut metadata = vec![("core", core), ("length", length), ("origins", origins)];
                if name == "LBORRESU" {
                    let mut entries = Vec::new();
                    for written in written(case) {
                        let id = a.text(written.id);
                        let terms = written
                            .clause
                            .iter()
                            .map(|(column, values)| (*column, a.strings(values)))
                            .collect();
                        let clause = a.map(terms);
                        let origin = a.texts(&[("type", "Assigned"), ("source", written.source)]);
                        let origins = a.node(N::Sequence(vec![origin]));
                        let method = a.text("Units assigned from the supplied test metadata");
                        let mut fields = vec![
                            ("id", id),
                            ("where", clause),
                            ("origins", origins),
                            ("method", method),
                        ];
                        if matches!(
                            case,
                            "stable values" | "reordered values" | "inserted value"
                        ) {
                            let comment = a.text("Independent value metadata comment");
                            fields.push(("comment", comment));
                        }
                        if case == "labelled" && written.id == "GLUC" {
                            let label = a.text("Glucose units");
                            fields.push(("label", label));
                        }
                        entries.push(a.map(fields));
                    }
                    let entries = a.node(N::Sequence(entries));
                    metadata.push(("value_level", entries));
                }
                let submission = a.map(metadata);
                fields.push(("submission", submission));
                columns.push(a.map(fields));
            }
            let columns = a.node(N::Sequence(columns));
            let names = a.strings(&["LBTESTCD", "LBSPEC", "LBSEQ", "LBORRESU"]);
            let output_path = a.text("lb.csv");
            let output = a.map(vec![("path", output_path), ("columns", names)]);
            let mut rows = Vec::new();
            for (id, code) in [
                ("glucose", "GLUC"),
                ("creatinine", "CREAT"),
                ("albumin", "ALB"),
            ] {
                let id = a.text(id);
                let literal = a.text(&format!("'{code}'"));
                let literal = a.map(vec![("value", literal)]);
                let derivations = a.map(vec![("LBTESTCD", literal)]);
                rows.push(a.map(vec![("id", id), ("derivations", derivations)]));
            }
            let rows = a.node(N::Sequence(rows));
            let root = a.map(vec![
                ("yamaa", yamaa),
                ("type", kind),
                ("model", model),
                ("domain", domain),
                ("submission", dataset),
                ("columns", columns),
                ("output", output),
                ("rows", rows),
            ]);
            a.finish(root)
        }
        fn compose(document: &Document) -> Study {
            let project = v1::project::project(&project(), None).unwrap();
            let declared = project.submission.unwrap();
            let section = &declared.sections[0];
            let required =
                submission_composition::Required::admit(declared.study.as_ref(), section).unwrap();
            let tree = v1::spec::lower(document, None).into_spec();
            submission_composition::compose(
                required,
                &Catalogue::admit(vec![]).unwrap(),
                &[Input {
                    document,
                    tree: &tree,
                    source: "spec.yaml",
                    artifact: "lb.csv",
                    href: Some("lb.csv"),
                }],
                &[],
                "0.3.0",
                "2026-09-11T00:00:00",
            )
            .unwrap()
        }
        fn render(case: &str) -> String {
            String::from_utf8(
                yamaa_adapters::submission_xml::render(&compose(&specification(case))).unwrap(),
            )
            .unwrap()
        }

        /// Seam: the encoder over a study composed in memory, as `render`
        /// composes it.
        #[test]
        fn an_entry_label_describes_its_definition() {
            let xml = render("labelled");
            let start = xml.find("<ItemDef OID=\"IT.LB.LBORRESU.GLUC\"").unwrap();
            let item = &xml[start..start + xml[start..].find("</ItemDef>").unwrap()];
            assert!(item.contains(">Glucose units</TranslatedText>"));
            let start = xml.find("<ItemDef OID=\"IT.LB.LBORRESU.CREAT\"").unwrap();
            let item = &xml[start..start + xml[start..].find("</ItemDef>").unwrap()];
            assert!(item.contains(">Original units</TranslatedText>"));
        }
        /// Seam: the encoder over a study composed in memory, as `render`
        /// composes it; it also compares the order of elements of different
        /// names, which no `select` path states.
        #[test]
        fn fixed_time_xml_has_one_column_list_two_ordered_item_and_where_references() {
            let xml = render("valid");
            assert!(xml.contains("CreationDateTime=\"2026-09-11T00:00:00\""));
            assert!(xml.contains("SourceSystemVersion=\"1.0\""));
            assert_eq!(xml.matches("<def:ValueListDef ").count(), 1);
            assert_eq!(xml.matches("<def:ValueListRef ").count(), 1);
            assert_eq!(xml.matches("<def:WhereClauseDef ").count(), 2);
            assert_eq!(xml.matches("<ItemDef ").count(), 6);
            assert!(xml.contains("<def:ValueListRef ValueListOID=\"VL.LB.LBORRESU\"/>"));
            for (index, code, source) in [(1, "GLUC", "Vendor"), (2, "CREAT", "Sponsor")] {
                let identity = format!("LB.LBORRESU.{code}");
                assert!(xml.contains(&format!("ItemOID=\"IT.{identity}\" OrderNumber=\"{index}\" Mandatory=\"No\" MethodOID=\"MT.{identity}\"")));
                assert!(xml.contains(&format!(
                    "<def:WhereClauseRef WhereClauseOID=\"WC.{identity}\"/>"
                )));
                assert!(xml.contains(&format!("<CheckValue>{code}</CheckValue>")));
                assert!(xml.contains(&format!(
                    "<def:Origin Type=\"Assigned\" Source=\"{source}\"/>"
                )));
                assert!(xml.contains(&format!("<MethodDef OID=\"MT.{identity}\"")));
            }
            assert_eq!(
                xml.matches("Comparator=\"EQ\" SoftHard=\"Soft\" def:ItemOID=\"IT.LB.LBTESTCD\"")
                    .count(),
                2
            );
            assert!(
                xml.find("<def:ValueListDef ").unwrap() < xml.find("<def:WhereClauseDef ").unwrap()
            );
            assert!(
                xml.find("<def:WhereClauseDef ").unwrap() < xml.find("<ItemGroupDef ").unwrap()
            );
            let start = xml.find("<ItemDef OID=\"IT.LB.LBORRESU\"").unwrap();
            let item = &xml[start..start + xml[start..].find("</ItemDef>").unwrap()];
            assert!(item.find("<def:Origin ").unwrap() < item.find("<def:ValueListRef ").unwrap());
        }
        /// Seam: the encoder over a study composed in memory, as `render`
        /// composes it.
        #[test]
        fn several_values_are_a_membership_and_several_columns_one_range_check_each() {
            let xml = render("membership");
            assert!(xml.contains(concat!(
                "<def:WhereClauseDef OID=\"WC.LB.LBORRESU.CHEM\">\n",
                "        <RangeCheck Comparator=\"IN\" SoftHard=\"Soft\" def:ItemOID=\"IT.LB.LBTESTCD\">\n",
                "          <CheckValue>GLUC</CheckValue>\n",
                "          <CheckValue>CREAT</CheckValue>\n",
                "        </RangeCheck>\n",
                "      </def:WhereClauseDef>\n",
            )));
            let xml = render("conjunction");
            assert!(xml.contains(concat!(
                "<def:WhereClauseDef OID=\"WC.LB.LBORRESU.GLUCSER\">\n",
                "        <RangeCheck Comparator=\"EQ\" SoftHard=\"Soft\" def:ItemOID=\"IT.LB.LBTESTCD\">\n",
                "          <CheckValue>GLUC</CheckValue>\n",
                "        </RangeCheck>\n",
                "        <RangeCheck Comparator=\"EQ\" SoftHard=\"Soft\" def:ItemOID=\"IT.LB.LBSPEC\">\n",
                "          <CheckValue>SERUM</CheckValue>\n",
                "        </RangeCheck>\n",
                "      </def:WhereClauseDef>\n",
            )));
        }

        /// Seam: the encoder over three studies composed in memory, as
        /// `render` composes them.
        #[test]
        fn value_identifiers_survive_unrelated_insertion_and_entry_reordering() {
            let original = render("stable values");
            let reordered = render("reordered values");
            let inserted = render("inserted value");
            for xml in [&original, &reordered, &inserted] {
                for code in ["GLUC", "CREAT"] {
                    for prefix in ["IT", "MT", "COM", "WC"] {
                        let oid = format!("{prefix}.LB.LBORRESU.{code}");
                        assert_eq!(xml.matches(&format!(" OID=\"{oid}\"")).count(), 1);
                    }
                    assert!(xml.contains(&format!(
                        "<def:WhereClauseRef WhereClauseOID=\"WC.LB.LBORRESU.{code}\"/>"
                    )));
                }
                assert_eq!(xml.matches(" OID=\"VL.LB.LBORRESU\"").count(), 1);
            }
            assert!(original.contains("ItemOID=\"IT.LB.LBORRESU.GLUC\" OrderNumber=\"1\""));
            assert!(reordered.contains("ItemOID=\"IT.LB.LBORRESU.GLUC\" OrderNumber=\"2\""));
            assert!(inserted.contains("ItemOID=\"IT.LB.LBORRESU.GLUC\" OrderNumber=\"2\""));
            assert!(inserted.contains("ItemOID=\"IT.LB.LBORRESU.ALB\" OrderNumber=\"1\""));
        }

        /// Seam: the encoder over a study composed in memory, as `render`
        /// composes it.
        #[test]
        fn where_values_keep_their_text_with_xml_escaping() {
            let xml = render("escaped value");
            assert!(xml.contains("ItemOID=\"IT.LB.LBORRESU.GLUC\""));
            assert!(xml.contains("<CheckValue>A.&amp;\" /\u{e9}</CheckValue>"));
        }
    }

    /// XPT-10 over held published rows, after core admitted every value and
    /// measured the transport lengths.
    mod xpt {
        use super::fixture::*;
        use yamaa_adapters::submission_xpt::{admit, document};
        use yamaa_core::{
            diagnostic::ConditionCode,
            json::Value as JsonValue,
            project_terminology as ct, submission_transport,
            value::{ColumnType, Value},
        };

        /// Seam: the transport encoder's admission over held rows, and over
        /// a table double declaring rows enough for five billion bytes, which
        /// no staged file holds.
        #[test]
        fn a_blank_last_row_or_a_file_past_five_billion_bytes_is_refused_before_writing() {
            let empty = ct::Catalogue::admit(vec![]).unwrap();
            let mut project = project();
            project.set(&["sdtm", "transport"], Tree::text("xpt"));
            let mut measured = one(project, specification(), &empty).unwrap();
            let held = rows(
                ColumnType::Str,
                vec![Value::Str("A".into()), Value::Missing],
            );
            submission_transport::measure(&mut measured, &[Some(&held)]).unwrap();
            let dataset = &measured.datasets()[0];
            let document = document(&measured, dataset);
            assert_eq!(document.variables[0].width, 1);
            let text = |value: &str| JsonValue::String(value.into());
            let finding = admit(&document, dataset, &held).unwrap_err();
            assert_eq!(finding.code, ConditionCode::XptLastRowBlank);
            assert_eq!(finding.definition().rule, Some("XPT-10"));
            assert_eq!(finding.path, "datasets.D0.artifact");
            assert_eq!(
                finding.context,
                [
                    ("artifact".into(), text("d0.csv")),
                    ("dataset".into(), text("D0")),
                    ("detail".into(), text("the last row encodes to blanks only")),
                    ("row".into(), JsonValue::from(1usize)),
                ]
                .into()
            );
            // One byte an observation: 640 header bytes, one NAMESTR padded
            // to 160, the 80-byte observation header and the observations.
            let wide = Rows {
                rows: 5_000_000_000 / document.observation(),
                ..rows(ColumnType::Str, vec![])
            };
            let finding = admit(&document, dataset, &wide).unwrap_err();
            assert_eq!(finding.code, ConditionCode::XptFileTooLarge);
            assert_eq!(finding.definition().rule, Some("XPT-10"));
            assert_eq!(finding.path, "datasets.D0.artifact");
            assert_eq!(
                finding.context,
                [
                    ("artifact".into(), text("d0.csv")),
                    ("bytes".into(), JsonValue::from(5_000_000_880u64)),
                    ("dataset".into(), text("D0")),
                    (
                        "detail".into(),
                        text("transport file larger than 5000000000 bytes")
                    ),
                ]
                .into()
            );
            // A last row that is not blank is admitted, and so is a file of
            // exactly 5,000,000,000 bytes.
            let present = rows(
                ColumnType::Str,
                vec![Value::Missing, Value::Str("A".into())],
            );
            assert_eq!(admit(&document, dataset, &present), Ok(()));
            let bound = Rows {
                rows: 5_000_000_000 - 880,
                ..rows(ColumnType::Str, vec![])
            };
            assert_eq!(document.file_bytes(bound.rows), 5_000_000_000);
            assert_eq!(admit(&document, dataset, &bound), Ok(()));
        }
    }
}
