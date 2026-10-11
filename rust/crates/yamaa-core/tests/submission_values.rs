//! Independently authored normalized metadata; no generator supplies expectations.
use yamaa_core::{
    project_terminology::Catalogue,
    schema::{Document, DocumentLimits, DocumentNode as N},
    specification_submission_document as declarations,
    submission_composition::{self, Input, Study},
    submission_document, submission_xml,
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
    fn finish(self, root: usize) -> Document {
        Document::new(self.0, root, DocumentLimits::default()).unwrap()
    }
}
fn environment() -> Document {
    let mut a = Arena::default();
    let study = a.texts(&[
        ("id", "VLSTUDY"),
        ("name", "Value metadata study"),
        ("description", "Independent structure witness"),
        ("protocol_name", "VLPROTOCOL"),
    ]);
    let standard = a.texts(&[("name", "SDTMIG"), ("version", "3.4"), ("status", "Final")]);
    let filename = a.text("spec.yaml");
    let specs = a.node(N::Sequence(vec![filename]));
    let define = a.text("define.xml");
    let version = a.text("1.0");
    let section = a.map(vec![
        ("standard", standard),
        ("specs", specs),
        ("define", define),
        ("source_system_version", version),
    ]);
    let root = a.map(vec![("study", study), ("sdtm", section)]);
    a.finish(root)
}
fn specification(case: &str) -> Document {
    let mut a = Arena::default();
    let domain = a.text(if case == "no discriminator" {
        "XX"
    } else {
        "LB"
    });
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
    for name in ["LBTESTCD", "LBORRESU"] {
        let origin = a.texts(&[("type", "Assigned"), ("source", "Sponsor")]);
        let core = a.text("Exp");
        let length = a.node(N::Integer("8".into()));
        let submission = a.map(vec![("core", core), ("length", length), ("origin", origin)]);
        let name_node = a.text(name);
        let kind = a.text("str");
        let label = a.text(if name == "LBORRESU" {
            "Original units"
        } else {
            "Test code"
        });
        let mut fields = vec![
            ("name", name_node),
            ("type", kind),
            ("label", label),
            ("submission", submission),
        ];
        if name == "LBORRESU" && case != "no derivation" {
            let literal = a.texts(&[("literal", "mg/dL")]);
            fields.push(("derivation", literal));
        }
        columns.push(a.map(fields));
    }
    let columns = a.node(N::Sequence(columns));
    let mut names = vec![a.text("LBTESTCD")];
    if case != "not output" {
        names.push(a.text("LBORRESU"));
    }
    let output_names = a.node(N::Sequence(names));
    let output_path = a.text("lb.csv");
    let output = a.map(vec![("path", output_path), ("columns", output_names)]);
    let mut rows = Vec::new();
    let row_order: &[usize] = match case {
        "reordered values" => &[1, 0],
        "inserted value" => &[2, 0, 1],
        _ => &[0, 1],
    };
    for &index in row_order {
        let id = a.text(match (index, case) {
            (0, "arbitrary row ID" | "arbitrary invalid source") => "glucose / primary test: 1",
            (0, "empty row ID") => "",
            (0, _) => "glucose",
            (1, _) => "creatinine",
            _ => "albumin",
        });
        let literal = if case == "dynamic discriminator" && index == 0 {
            a.texts(&[("source", "ODM.TestCode")])
        } else if case == "ambiguous discriminator" && index == 0 {
            a.texts(&[("literal", "GLUC"), ("source", "ODM.TestCode")])
        } else if case == "nonstring discriminator" && index == 0 {
            let number = a.node(N::Integer("1".into()));
            a.map(vec![("literal", number)])
        } else {
            a.texts(&[(
                "literal",
                if index == 0 && case == "escaped test code" {
                    "A.&\" /é"
                } else if index == 0 || matches!(case, "conflict" | "identical") {
                    "GLUC"
                } else if index == 1 {
                    "CREAT"
                } else {
                    "ALB"
                },
            )])
        };
        let literal = if case == "normalized discriminator" {
            let unconvertible = a.node(N::Null);
            a.map(vec![("value", literal), ("unconvertible", unconvertible)])
        } else {
            literal
        };
        let derivations = a.map(vec![("LBTESTCD", literal)]);
        let origin_type = if case == "refuted origin" && index == 0 {
            "Collected"
        } else {
            "Assigned"
        };
        let source = if (case == "invalid source" && index == 1)
            || (case == "arbitrary invalid source" && index == 0)
        {
            "Investigator"
        } else if index == 0 || case == "identical" {
            "Vendor"
        } else {
            "Sponsor"
        };
        let origin = a.texts(&[("type", origin_type), ("source", source)]);
        let method = a.text("Units assigned from the supplied test metadata");
        let mut metadata_fields = vec![("origin", origin), ("method", method)];
        if matches!(
            case,
            "stable values" | "reordered values" | "inserted value"
        ) {
            let comment = a.text("Independent value metadata comment");
            metadata_fields.push(("comment", comment));
        }
        let metadata = a.map(metadata_fields);
        let submission = a.map(vec![(
            if case == "unknown column" && index == 0 {
                "BOGUS"
            } else {
                "LBORRESU"
            },
            metadata,
        )]);
        rows.push(a.map(vec![
            ("id", id),
            ("derivations", derivations),
            ("submission", submission),
        ]));
    }
    let rows = a.node(N::Sequence(rows));
    let root = a.map(vec![
        ("domain", domain),
        ("submission", dataset),
        ("columns", columns),
        ("output", output),
        ("rows", rows),
    ]);
    a.finish(root)
}
fn compose(document: &Document) -> Result<Study, Vec<submission_composition::Finding>> {
    let environment = environment();
    let declared =
        submission_document::decode(&environment, environment.root(), Default::default()).unwrap();
    submission_composition::compose(
        declared.study().unwrap(),
        &declared.sections()[0],
        &Catalogue::admit(vec![]).unwrap(),
        &[Input {
            document,
            source: "spec.yaml",
            artifact: "lb.csv",
            href: "lb.csv",
        }],
        "0.3.0",
        "2026-09-11T00:00:00",
    )
}

#[test]
fn overrides_inherit_complete_metadata_and_preserve_written_occurrences() {
    let document = specification("valid");
    let declared = declarations::decode(&document, document.root(), Default::default()).unwrap();
    assert_eq!(declared.value_rows().len(), 2);
    let row = &declared.value_rows()[0];
    assert_eq!(row.id.value, "glucose");
    assert_eq!(document.field(row.node, "submission"), row.origin.node);
    assert_eq!(row.overrides[0].name.value, "LBORRESU");
    assert!(row.overrides[0].submission.core.is_none());
    let study = compose(&document).unwrap();
    let column = &study.datasets()[0].columns()[1];
    assert_eq!(column.values().len(), 2);
    let value = &column.values()[0];
    assert_eq!(value.test_code(), "GLUC");
    let metadata = value.column().declaration().submission.as_ref().unwrap();
    assert_eq!(
        metadata.core.as_ref().unwrap().value,
        declarations::Core::Expected
    );
    assert_eq!(metadata.length.as_ref().unwrap().value, "8");
    assert_eq!(
        metadata
            .origin_metadata
            .as_ref()
            .unwrap()
            .source
            .as_ref()
            .unwrap()
            .value,
        declarations::OriginSource::Vendor
    );
}
#[test]
fn complete_row_metadata_is_counted_before_ownership_or_findings() {
    let document = specification("valid");
    let limits = declarations::Limits {
        declarations: 4,
        ..Default::default()
    };
    let error = declarations::decode(&document, document.root(), limits).unwrap_err();
    assert!(std::ptr::eq(error.input, &document));
    assert_eq!(
        error.cause,
        declarations::Cause::Limit(declarations::Resource::Declarations)
    );
    // Dataset + two columns + two row overrides are all admitted at once.
    let limits = declarations::Limits {
        declarations: 5,
        ..Default::default()
    };
    assert_eq!(
        declarations::decode(&document, document.root(), limits)
            .unwrap()
            .value_rows()
            .len(),
        2
    );
}
#[test]
fn arbitrary_row_labels_and_normalized_literal_handlers_retain_their_meaning() {
    for case in ["arbitrary row ID", "normalized discriminator"] {
        let document = specification(case);
        let declared =
            declarations::decode(&document, document.root(), Default::default()).unwrap();
        let study = compose(&document).unwrap();
        let value = &study.datasets()[0].columns()[1].values()[0];
        assert_eq!(value.test_code(), "GLUC");
        if case == "arbitrary row ID" {
            assert_eq!(
                declared.value_rows()[0].id.value,
                "glucose / primary test: 1"
            );
        }
    }
    let findings = compose(&specification("arbitrary invalid source")).unwrap_err();
    assert!(findings.iter().any(|finding| finding.definition.condition
        == "origin_source_not_admitted"
        && finding
            .path
            .ends_with("rows.glucose / primary test: 1.submission.LBORRESU.origin")));
}
#[test]
fn an_empty_row_label_is_a_text_refusal_with_its_original_field_occurrence() {
    let document = specification("empty row ID");
    let error = declarations::decode(&document, document.root(), Default::default()).unwrap_err();
    assert!(std::ptr::eq(error.input, &document));
    let declarations::Cause::Findings(findings) = error.cause else {
        panic!("expected a structural text refusal");
    };
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].kind, declarations::Kind::EmptyText);
    assert!(findings[0].requirement().is_none());
    assert_eq!(
        findings[0].path.steps().collect::<Vec<_>>(),
        [
            declarations::Step::Field(declarations::FieldName::Rows),
            declarations::Step::Index(0),
            declarations::Step::Field(declarations::FieldName::Id),
        ]
    );
}
#[test]
fn each_independent_value_admission_control_has_the_required_condition_and_row_path() {
    for (case, condition, requirement, suffix) in [
        (
            "no discriminator",
            "value_metadata_requires_testcd",
            "REQ-1163",
            "rows.glucose.submission",
        ),
        (
            "dynamic discriminator",
            "value_metadata_requires_literal_testcd",
            "REQ-1164",
            "rows.glucose.submission",
        ),
        (
            "ambiguous discriminator",
            "value_metadata_requires_literal_testcd",
            "REQ-1164",
            "rows.glucose.submission",
        ),
        (
            "nonstring discriminator",
            "value_metadata_requires_literal_testcd",
            "REQ-1164",
            "rows.glucose.submission",
        ),
        (
            "unknown column",
            "value_metadata_unknown_column",
            "REQ-1165",
            "rows.glucose.submission.BOGUS",
        ),
        (
            "no derivation",
            "value_metadata_unknown_column",
            "REQ-1165",
            "rows.glucose.submission.LBORRESU",
        ),
        (
            "not output",
            "value_metadata_not_output_column",
            "REQ-1166",
            "rows.glucose.submission.LBORRESU",
        ),
        (
            "conflict",
            "conflicting_value_metadata",
            "REQ-1167",
            "rows.creatinine.submission.LBORRESU",
        ),
        (
            "refuted origin",
            "refuted_value_origin",
            "REQ-1168",
            "rows.glucose.submission.LBORRESU",
        ),
        (
            "invalid source",
            "origin_source_not_admitted",
            "REQ-0919",
            "rows.creatinine.submission.LBORRESU.origin",
        ),
    ] {
        let findings = compose(&specification(case)).unwrap_err();
        assert!(
            findings
                .iter()
                .any(|finding| finding.definition.condition == condition
                    && finding.definition.requirement == Some(requirement)
                    && finding.path.ends_with(suffix)),
            "{case}: {findings:?}"
        );
    }
}
#[test]
fn identical_declarations_at_distinct_nodes_share_one_value_definition() {
    let document = specification("identical");
    let declared = declarations::decode(&document, document.root(), Default::default()).unwrap();
    let a = &declared.value_rows()[0].overrides[0].submission;
    let b = &declared.value_rows()[1].overrides[0].submission;
    assert_ne!(a, b, "distinct occurrence facts are retained");
    assert!(a.same_values(b));
    let study = compose(&document).unwrap();
    assert_eq!(study.datasets()[0].columns()[1].values().len(), 1);
}
#[test]
fn fixed_time_xml_has_one_column_list_two_ordered_item_and_where_references() {
    let study = compose(&specification("valid")).unwrap();
    let xml = String::from_utf8(submission_xml::render(&study).unwrap()).unwrap();
    assert!(xml.contains("CreationDateTime=\"2026-09-11T00:00:00\""));
    assert!(xml.contains("SourceSystemVersion=\"1.0\""));
    assert_eq!(xml.matches("<def:ValueListDef ").count(), 1);
    assert_eq!(xml.matches("<def:ValueListRef ").count(), 1);
    assert_eq!(xml.matches("<def:WhereClauseDef ").count(), 2);
    assert_eq!(xml.matches("<ItemDef ").count(), 4);
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
    assert!(xml.find("<def:ValueListDef ").unwrap() < xml.find("<def:WhereClauseDef ").unwrap());
    assert!(xml.find("<def:WhereClauseDef ").unwrap() < xml.find("<ItemGroupDef ").unwrap());
    let start = xml.find("<ItemDef OID=\"IT.LB.LBORRESU\"").unwrap();
    let item = &xml[start..start + xml[start..].find("</ItemDef>").unwrap()];
    assert!(item.find("<def:Origin ").unwrap() < item.find("<def:ValueListRef ").unwrap());
}

#[test]
fn value_identifiers_survive_unrelated_insertion_and_row_reordering() {
    let render = |case| {
        String::from_utf8(submission_xml::render(&compose(&specification(case)).unwrap()).unwrap())
            .unwrap()
    };
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

#[test]
fn value_identifier_retains_the_literal_with_xml_attribute_escaping() {
    let xml = String::from_utf8(
        submission_xml::render(&compose(&specification("escaped test code")).unwrap()).unwrap(),
    )
    .unwrap();
    assert!(xml.contains("ItemOID=\"IT.LB.LBORRESU.A.&amp;&quot; /é\""));
    assert!(xml.contains(" OID=\"WC.LB.LBORRESU.A.&amp;&quot; /é\""));
    assert!(xml.contains("<CheckValue>A.&amp;\" /é</CheckValue>"));
}
