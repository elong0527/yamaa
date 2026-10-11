use yamaa_core::{
    project_environment::Submission,
    schema::{Document, DocumentLimits, DocumentNode as N},
    submission_document::{self, Cause, Limits, Resource},
};

fn declarations() -> Document {
    // Each value is a distinct occurrence. Null values deliberately establish
    // locations only; no submission validation/acceptance is inferred here.
    Document::new(
        vec![
            N::Text("send".into()),
            N::Null,
            N::Text("study".into()),
            N::Null,
            N::Text("adam".into()),
            N::Null,
            N::Text("sdtm".into()),
            N::Null,
            N::Text("language".into()),
            N::Text("r".into()),
            N::Mapping(vec![(0, 1), (2, 3), (4, 5), (6, 7), (8, 9)]),
        ],
        10,
        DocumentLimits::default(),
    )
    .unwrap()
}

#[test]
fn borrowed_declarations_keep_original_occurrences_and_authored_section_order() {
    let input = declarations();
    let facts = submission_document::locate(&input, 10, Limits::default()).unwrap();
    assert!(std::ptr::eq(facts.input(), &input));
    assert_eq!(facts.study(), Some(3));
    assert_eq!(
        facts.sections(),
        [
            (Submission::Send, 1),
            (Submission::Adam, 5),
            (Submission::Sdtm, 7)
        ]
    );
    assert!(matches!(facts.input().nodes()[3], N::Null));
}

#[test]
fn limited_location_scans_preserve_the_exact_input_without_partial_facts() {
    let input = declarations();
    for (limits, resource) in [
        (
            Limits {
                fields: 4,
                ..Limits::default()
            },
            Resource::Fields,
        ),
        (
            Limits {
                work: 0,
                ..Limits::default()
            },
            Resource::Work,
        ),
        (
            Limits {
                declarations: 2,
                ..Limits::default()
            },
            Resource::Declarations,
        ),
    ] {
        let error = submission_document::locate(&input, 10, limits).unwrap_err();
        assert!(std::ptr::eq(error.input, &input));
        assert_eq!(error.cause, Cause::Limit(resource));
        assert_eq!(error.input.field(10, "sdtm"), Some(7));
    }
}

#[test]
fn absent_declarations_and_original_shape_refusals_remain_distinct() {
    let empty = Document::new(vec![N::Mapping(vec![])], 0, DocumentLimits::default()).unwrap();
    let facts = submission_document::locate(&empty, 0, Limits::default()).unwrap();
    assert_eq!(facts.study(), None);
    assert!(facts.sections().is_empty());
    let input = Document::new(
        vec![N::Integer("1".into()), N::Null, N::Mapping(vec![(0, 1)])],
        2,
        DocumentLimits::default(),
    )
    .unwrap();
    for (root, node) in [(0, 0), (2, 0), (99, 99)] {
        let error = submission_document::locate(&input, root, Limits::default()).unwrap_err();
        assert!(std::ptr::eq(error.input, &input));
        assert_eq!(error.cause, Cause::Shape { node });
    }
}

use submission_document::{
    Context, DocumentKind, FieldName as F, GuideName, GuideStatus, Kind, Step,
};

// Independently authored arena fixtures; no decoder or schema normalization
// helper builds the expected values. Each child is a distinct occurrence.
#[derive(Default)]
struct Arena(Vec<N>);
impl Arena {
    fn node(&mut self, value: N) -> usize {
        let id = self.0.len();
        self.0.push(value);
        id
    }
    fn text(&mut self, value: &str) -> usize {
        self.node(N::Text(value.into()))
    }
    fn map(&mut self, fields: Vec<(&str, usize)>) -> usize {
        let fields = fields
            .into_iter()
            .map(|(name, node)| (self.text(name), node))
            .collect();
        self.node(N::Mapping(fields))
    }
    fn text_map(&mut self, fields: &[(&str, &str)]) -> usize {
        let fields = fields
            .iter()
            .map(|&(key, value)| (key, self.text(value)))
            .collect();
        self.map(fields)
    }
    fn guide(&mut self, name: &str) -> usize {
        self.text_map(&[("name", name), ("version", "3.4"), ("status", "Final")])
    }
    fn section(&mut self, guide: &str) -> usize {
        let guide = self.guide(guide);
        let one = self.text("specs/dm.yaml");
        let two = self.text("specs/ae.yaml");
        let specs = self.node(N::Sequence(vec![one, two]));
        let define = self.text("results/define.xml");
        self.map(vec![
            ("standard", guide),
            ("specs", specs),
            ("define", define),
        ])
    }
    fn finish(self, root: usize) -> Document {
        Document::new(self.0, root, DocumentLimits::default()).unwrap()
    }
}
fn full() -> Document {
    let mut a = Arena::default();
    let study = a.text_map(&[
        ("id", "STUDY.alpha-1"),
        ("name", "Independent study"),
        ("description", "A & B < C"),
        ("protocol_name", "P-001"),
    ]);
    let send = a.section("SENDIG-DART");
    let sdtm = a.section("SDTMIG-AP");
    let guide = a.guide("ADaMIG-MD");
    let spec = a.text("analysis/adsl.yaml");
    let specs = a.node(N::Sequence(vec![spec]));
    let define = a.text("analysis/define.xml");
    let docs = [
        (
            "review",
            "supplemental",
            "reviewers/review.pdf",
            "Reviewer guide",
        ),
        ("acrf", "annotated_crf", "forms/acrf.pdf", "Annotated CRF"),
        ("other", "other", "notes/data.txt", "Notes"),
    ]
    .into_iter()
    .map(|(id, kind, link, title)| {
        a.text_map(&[("id", id), ("kind", kind), ("link", link), ("title", title)])
    })
    .collect();
    let docs = a.node(N::Sequence(docs));
    let enabled = a.node(N::Boolean(true));
    let context = a.text("Other");
    let xml_lang = a.text("fr-CA");
    let file = a.text("FILE.analysis-1");
    let metadata = a.text_map(&[
        ("id", "MDV.analysis-1"),
        ("name", "Analysis metadata"),
        ("description", "Sponsor metadata"),
    ]);
    let originator = a.text("Independent sponsor");
    let style = a.text("styles/define.xsl");
    let adam = a.map(vec![
        ("documents", docs),
        ("originator", originator),
        ("standard", guide),
        ("specs", specs),
        ("define", define),
        ("dataset_json", enabled),
        ("context", context),
        ("xml_lang", xml_lang),
        ("file_oid", file),
        ("metadata_version", metadata),
        ("stylesheet", style),
    ]);
    let language = a.text("python");
    let root = a.map(vec![
        ("send", send),
        ("language", language),
        ("study", study),
        ("adam", adam),
        ("sdtm", sdtm),
    ]);
    a.finish(root)
}
#[test]
fn complete_owned_metadata_preserves_order_overrides_and_occurrences_after_input_drops() {
    let input = full();
    let root = input.root();
    let study_node = input.field(root, "study").unwrap();
    let id_node = input.field(study_node, "id").unwrap();
    let owned = submission_document::decode(&input, root, Limits::default()).unwrap();
    drop(input);
    assert_eq!(owned.root(), root);
    let study = owned.study().unwrap();
    assert_eq!(study.id.value, "STUDY.alpha-1");
    assert_eq!(study.name.value, "Independent study");
    assert_eq!(study.description.value, "A & B < C");
    assert_eq!(study.protocol_name.value, "P-001");
    assert_eq!(study.origin.node, Some(study_node));
    assert_eq!(study.id.origin.container, study_node);
    assert_eq!(study.id.origin.node, Some(id_node));
    assert_eq!(study.id.origin.field, F::Id);
    assert_eq!(
        owned
            .sections()
            .iter()
            .map(|s| s.family)
            .collect::<Vec<_>>(),
        [Submission::Send, Submission::Adam, Submission::Sdtm]
    );
    let send = &owned.sections()[0];
    assert_eq!(send.standard.name.value, GuideName::SendIgDart);
    assert_eq!(send.standard.version.value, "3.4");
    assert_eq!(send.standard.status.value, GuideStatus::Final);
    assert_eq!(
        send.specs
            .value
            .iter()
            .map(|s| s.value.as_str())
            .collect::<Vec<_>>(),
        ["specs/dm.yaml", "specs/ae.yaml"]
    );
    assert_eq!(send.define.value, "results/define.xml");
    assert!(send.documents.value.is_empty());
    assert!(!send.dataset_json.value);
    assert_eq!(send.context.value, Context::Submission);
    assert_eq!(send.xml_lang.value, "en");
    for origin in [
        send.documents.origin,
        send.dataset_json.origin,
        send.context.origin,
        send.xml_lang.origin,
    ] {
        assert_eq!(origin.node, None);
        assert_eq!(origin.key, None);
        assert_eq!(origin.container, send.origin.node.unwrap());
    }
    assert!(
        send.file_oid.is_none()
            && send.metadata_version.is_none()
            && send.originator.is_none()
            && send.stylesheet.is_none()
    );
    let adam = &owned.sections()[1];
    assert_eq!(adam.standard.name.value, GuideName::AdamIgMd);
    assert!(adam.dataset_json.value);
    assert_eq!(adam.context.value, Context::Other);
    assert_eq!(adam.xml_lang.value, "fr-CA");
    assert_eq!(adam.file_oid.as_ref().unwrap().value, "FILE.analysis-1");
    let metadata = adam.metadata_version.as_ref().unwrap();
    assert_eq!(metadata.id.value, "MDV.analysis-1");
    assert_eq!(metadata.name.value, "Analysis metadata");
    assert_eq!(
        metadata.description.as_ref().unwrap().value,
        "Sponsor metadata"
    );
    assert_eq!(
        adam.originator.as_ref().unwrap().value,
        "Independent sponsor"
    );
    assert_eq!(adam.stylesheet.as_ref().unwrap().value, "styles/define.xsl");
    assert_eq!(
        adam.documents
            .value
            .iter()
            .map(|d| (
                d.id.value.as_str(),
                d.kind.value,
                d.link.value.as_str(),
                d.title.value.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            (
                "review",
                DocumentKind::Supplemental,
                "reviewers/review.pdf",
                "Reviewer guide"
            ),
            (
                "acrf",
                DocumentKind::AnnotatedCrf,
                "forms/acrf.pdf",
                "Annotated CRF"
            ),
            ("other", DocumentKind::Other, "notes/data.txt", "Notes"),
        ]
    );
    assert_eq!(owned.sections()[2].standard.name.value, GuideName::SdtmIgAp);
}
#[test]
fn every_shipped_guide_spelling_is_closed_metadata_not_family_composition() {
    for (text, expected) in [
        ("ADaMIG", GuideName::AdamIg),
        ("ADaMIG-MD", GuideName::AdamIgMd),
        ("BIMO", GuideName::Bimo),
        ("CDISC/NCI", GuideName::CdiscNci),
        ("SDTMIG", GuideName::SdtmIg),
        ("SDTMIG-AP", GuideName::SdtmIgAp),
        ("SDTMIG-MD", GuideName::SdtmIgMd),
        ("SENDIG", GuideName::SendIg),
        ("SENDIG-AR", GuideName::SendIgAr),
        ("SENDIG-DART", GuideName::SendIgDart),
        ("SENDIG-GENETOX", GuideName::SendIgGenetox),
    ] {
        let mut a = Arena::default();
        let section = a.section(text);
        let root = a.map(vec![("sdtm", section)]);
        let input = a.finish(root);
        let owned = submission_document::decode(&input, root, Limits::default()).unwrap();
        assert_eq!(owned.sections()[0].standard.name.value, expected);
        assert_eq!(owned.sections()[0].standard.name.value.name(), text);
        assert!(owned.study().is_none()); // missing generator prerequisites remain a separate use case.
    }
}
#[test]
fn absent_declarations_and_explicit_default_values_have_distinct_origin_facts() {
    let mut a = Arena::default();
    let language = a.text("r");
    let root = a.map(vec![("language", language)]);
    let input = a.finish(root);
    let owned = submission_document::decode(&input, root, Limits::default()).unwrap();
    assert!(owned.study().is_none());
    assert!(owned.sections().is_empty());
    let mut a = Arena::default();
    let guide = a.guide("SDTMIG");
    let specs = a.node(N::Sequence(vec![]));
    let define = a.text("define.xml");
    let documents = a.node(N::Sequence(vec![]));
    let flag = a.node(N::Boolean(false));
    let context = a.text("Submission");
    let lang = a.text("en");
    let section = a.map(vec![
        ("standard", guide),
        ("specs", specs),
        ("define", define),
        ("documents", documents),
        ("dataset_json", flag),
        ("context", context),
        ("xml_lang", lang),
    ]);
    let root = a.map(vec![("sdtm", section)]);
    let input = a.finish(root);
    let owned = submission_document::decode(&input, root, Limits::default()).unwrap();
    let s = &owned.sections()[0];
    for (origin, expected) in [
        (s.documents.origin, documents),
        (s.dataset_json.origin, flag),
        (s.context.origin, context),
        (s.xml_lang.origin, lang),
    ] {
        assert_eq!(origin.node, Some(expected));
        assert!(origin.key.is_some());
    }
    assert!(s.specs.value.is_empty());
}
fn invalid() -> Document {
    let mut a = Arena::default();
    let study = a.text_map(&[
        ("id", "9 bad"),
        ("name", ""),
        ("description", "Description"),
        ("protocol_name", "P"),
    ]);
    let guide = a.text_map(&[("name", "UNKNOWN"), ("version", ""), ("status", "final")]);
    let spec = a.node(N::Integer("99".into()));
    let specs = a.node(N::Sequence(vec![spec]));
    let define = a.node(N::Null);
    let doc = a.text_map(&[
        ("id", "bad.id"),
        ("kind", "wrong"),
        ("link", "docs/../escape.pdf"),
        ("title", ""),
    ]);
    let documents = a.node(N::Sequence(vec![doc]));
    let flag = a.text("true");
    let context = a.text("submission");
    let language = a.text("e-123456789");
    let file = a.text("9FILE");
    let metadata = a.text_map(&[("id", "MDV"), ("name", ""), ("description", "")]);
    let originator = a.node(N::Boolean(true));
    let style = a.text("/absolute.xsl");
    let unknown = a.text("1.0");
    let section = a.map(vec![
        ("standard", guide),
        ("specs", specs),
        ("define", define),
        ("documents", documents),
        ("dataset_json", flag),
        ("context", context),
        ("xml_lang", language),
        ("file_oid", file),
        ("metadata_version", metadata),
        ("originator", originator),
        ("stylesheet", style),
        ("generator_version", unknown),
    ]);
    let root = a.map(vec![("study", study), ("adam", section)]);
    a.finish(root)
}
#[test]
fn structural_findings_are_complete_ordered_and_only_supported_causes_are_portable() {
    let input = invalid();
    let error = submission_document::decode(&input, input.root(), Limits::default()).unwrap_err();
    assert!(std::ptr::eq(error.input, &input));
    let Cause::Findings(findings) = error.cause else {
        panic!("expected complete structural findings")
    };
    assert_eq!(
        findings.iter().map(|f| f.kind).collect::<Vec<_>>(),
        [
            Kind::InvalidDefineId,
            Kind::EmptyText,
            Kind::UnknownField,
            Kind::InvalidChoice,
            Kind::EmptyText,
            Kind::InvalidChoice,
            Kind::ExpectedText,
            Kind::ExpectedText,
            Kind::InvalidIdentifier,
            Kind::InvalidChoice,
            Kind::InvalidHref,
            Kind::EmptyText,
            Kind::ExpectedBoolean,
            Kind::InvalidChoice,
            Kind::InvalidLanguage,
            Kind::InvalidDefineId,
            Kind::EmptyText,
            Kind::EmptyText,
            Kind::ExpectedText,
            Kind::InvalidHref,
        ]
    );
    assert_eq!(
        findings[0].path.steps().collect::<Vec<_>>(),
        [Step::Field(F::Study), Step::Field(F::Id)]
    );
    assert_eq!(
        findings[10].path.steps().collect::<Vec<_>>(),
        [
            Step::Field(F::Adam),
            Step::Field(F::Documents),
            Step::Index(0),
            Step::Field(F::Link)
        ]
    );
    assert_eq!(
        findings
            .iter()
            .filter_map(|f| f.requirement())
            .collect::<Vec<_>>(),
        ["REQ-1068", "REQ-1074", "REQ-1046", "REQ-1070", "REQ-1073", "REQ-1068", "REQ-1070"]
    );
    assert_eq!(findings[7].requirement(), None); // null is an original shape, not a fabricated normative message.
    assert!(matches!(error.input.nodes()[findings[7].node], N::Null));
}
#[test]
fn full_finding_and_path_limits_refuse_instead_of_reporting_a_fitting_prefix() {
    let input = invalid();
    for (limits, resource) in [
        (
            Limits {
                findings: 19,
                ..Limits::default()
            },
            Resource::Findings,
        ),
        (
            Limits {
                path_segments: 0,
                ..Limits::default()
            },
            Resource::PathSegments,
        ),
    ] {
        let error = submission_document::decode(&input, input.root(), limits).unwrap_err();
        assert!(std::ptr::eq(error.input, &input));
        assert_eq!(error.cause, Cause::Limit(resource));
        assert!(error.input.field(error.input.root(), "adam").is_some());
    }
    assert!(
        matches!(submission_document::decode(&input, input.root(), Limits { findings: 20, ..Limits::default() }).unwrap_err().cause, Cause::Findings(v) if v.len() == 20)
    );
}
#[test]
fn whole_model_limits_keep_original_input_without_partial_owned_results() {
    let input = full();
    for (limits, resource) in [
        (
            Limits {
                fields: 1,
                ..Limits::default()
            },
            Resource::Fields,
        ),
        (
            Limits {
                declarations: 3,
                ..Limits::default()
            },
            Resource::Declarations,
        ),
        (
            Limits {
                nodes: 1,
                ..Limits::default()
            },
            Resource::Nodes,
        ),
        (
            Limits {
                depth: 1,
                ..Limits::default()
            },
            Resource::Depth,
        ),
        (
            Limits {
                list_items: 1,
                ..Limits::default()
            },
            Resource::ListItems,
        ),
        (
            Limits {
                text_bytes: 1,
                ..Limits::default()
            },
            Resource::TextBytes,
        ),
        (
            Limits {
                escaped_bytes: 1,
                ..Limits::default()
            },
            Resource::EscapedBytes,
        ),
        (
            Limits {
                work: 1,
                ..Limits::default()
            },
            Resource::Work,
        ),
    ] {
        let error = submission_document::decode(&input, input.root(), limits).unwrap_err();
        assert!(std::ptr::eq(error.input, &input));
        assert_eq!(error.cause, Cause::Limit(resource));
    }
}
#[test]
fn null_optional_fields_are_original_shape_findings_not_absent_defaults() {
    let mut a = Arena::default();
    let guide = a.guide("SENDIG");
    let specs = a.node(N::Sequence(vec![]));
    let define = a.text("d.xml");
    let mut fields = vec![("standard", guide), ("specs", specs), ("define", define)];
    for key in [
        "documents",
        "dataset_json",
        "context",
        "xml_lang",
        "file_oid",
        "metadata_version",
        "originator",
        "stylesheet",
    ] {
        fields.push((key, a.node(N::Null)));
    }
    let section = a.map(fields);
    let root = a.map(vec![("send", section)]);
    let input = a.finish(root);
    let error = submission_document::decode(&input, root, Limits::default()).unwrap_err();
    let Cause::Findings(findings) = error.cause else {
        panic!("expected findings")
    };
    assert_eq!(
        findings.iter().map(|f| f.kind).collect::<Vec<_>>(),
        [
            Kind::ExpectedSequence,
            Kind::ExpectedBoolean,
            Kind::ExpectedText,
            Kind::ExpectedText,
            Kind::ExpectedText,
            Kind::ExpectedMapping,
            Kind::ExpectedText,
            Kind::ExpectedText
        ]
    );
    assert!(findings
        .iter()
        .all(|f| f.requirement().is_none() && matches!(input.nodes()[f.node], N::Null)));
}
#[test]
fn scalar_container_and_nontext_key_refusals_preserve_original_shapes() {
    for scalar in [
        N::Null,
        N::Boolean(true),
        N::Integer("9999999999999999999999999".into()),
        N::Float(1.5),
        N::Text("study".into()),
        N::Sequence(vec![]),
    ] {
        let mut a = Arena::default();
        let node = a.node(scalar);
        let root = a.map(vec![("study", node)]);
        let input = a.finish(root);
        let error = submission_document::decode(&input, root, Limits::default()).unwrap_err();
        assert!(std::ptr::eq(error.input, &input));
        assert!(
            matches!(error.cause, Cause::Findings(v) if v.len() == 1 && v[0].node == node && v[0].kind == Kind::ExpectedMapping && v[0].requirement().is_none())
        );
    }
    let mut a = Arena::default();
    let key = a.node(N::Boolean(true));
    let value = a.node(N::Null);
    let study = a.node(N::Mapping(vec![(key, value)]));
    let root = a.map(vec![("study", study)]);
    let input = a.finish(root);
    let Cause::Findings(findings) = submission_document::decode(&input, root, Limits::default())
        .unwrap_err()
        .cause
    else {
        panic!("findings")
    };
    assert_eq!(findings.len(), 5);
    assert_eq!(findings[0].node, key);
    assert_eq!(findings[0].kind, Kind::NonTextKey);
    assert!(findings[1..]
        .iter()
        .all(|f| f.kind == Kind::Missing && f.origin.unwrap().node.is_none()));
}
#[test]
fn document_transport_depth_and_decoder_depth_refusals_remain_distinct() {
    let mut nodes = vec![N::Text("original deep scalar".into())];
    let mut child = 0;
    for _ in 0..70 {
        nodes.push(N::Sequence(vec![child]));
        child = nodes.len() - 1;
    }
    let key = nodes.len();
    nodes.push(N::Text("study".into()));
    let root = nodes.len();
    nodes.push(N::Mapping(vec![(key, child)]));
    assert_eq!(
        Document::new(
            nodes,
            root,
            DocumentLimits {
                depth: 100,
                ..DocumentLimits::default()
            }
        )
        .unwrap_err(),
        yamaa_core::schema::DocumentError::Limit {
            resource: yamaa_core::schema::DocumentResource::Depth,
            limit: 64
        }
    );
    let mut a = Arena::default();
    let mut child = a.text("original scalar");
    for _ in 0..10 {
        child = a.node(N::Sequence(vec![child]));
    }
    let root = a.map(vec![("study", child)]);
    let input = a.finish(root);
    let error = submission_document::decode(
        &input,
        root,
        Limits {
            depth: 5,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.cause, Cause::Limit(Resource::Depth));
    assert!(std::ptr::eq(error.input, &input));
    let error = submission_document::decode(
        &input,
        root,
        Limits {
            depth: usize::MAX,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert!(
        matches!(error.cause, Cause::Findings(v) if v.len() == 1 && v[0].kind == Kind::ExpectedMapping)
    );
}

#[test]
fn unknown_fields_and_nontext_keys_retain_exact_container_and_key_occurrences() {
    let input = invalid();
    let root = input.root();
    let section = input.field(root, "adam").unwrap();
    let Cause::Findings(findings) = submission_document::decode(&input, root, Limits::default())
        .unwrap_err()
        .cause
    else {
        panic!("findings")
    };
    let unknown = &findings[2];
    assert_eq!(unknown.kind, Kind::UnknownField);
    assert_eq!(unknown.container, Some(section));
    assert!(
        matches!(&input.nodes()[unknown.key.unwrap()], N::Text(text) if text == "generator_version")
    );
    assert_eq!(
        unknown.node,
        input.field(section, "generator_version").unwrap()
    );
    assert_eq!(unknown.requirement(), None);
    let mut a = Arena::default();
    let key = a.node(N::Integer("2".into()));
    let value = a.node(N::Null);
    let root = a.node(N::Mapping(vec![(key, value)]));
    let input = a.finish(root);
    let Cause::Findings(findings) = submission_document::decode(&input, root, Limits::default())
        .unwrap_err()
        .cause
    else {
        panic!("findings")
    };
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].kind, Kind::NonTextKey);
    assert_eq!(findings[0].container, Some(root));
    assert_eq!(findings[0].key, Some(key));
    assert_eq!(findings[0].node, key);
}
#[test]
fn present_metadata_description_is_optional_and_draft_status_is_retained() {
    let mut a = Arena::default();
    let guide = a.text_map(&[("name", "ADaMIG"), ("version", "1.3"), ("status", "Draft")]);
    let specs = a.node(N::Sequence(vec![]));
    let define = a.text("define.xml");
    let metadata = a.text_map(&[("id", "MDV"), ("name", "Metadata")]);
    let section = a.map(vec![
        ("standard", guide),
        ("specs", specs),
        ("define", define),
        ("metadata_version", metadata),
    ]);
    let root = a.map(vec![("adam", section)]);
    let input = a.finish(root);
    let owned = submission_document::decode(&input, root, Limits::default()).unwrap();
    assert_eq!(
        owned.sections()[0].standard.status.value,
        GuideStatus::Draft
    );
    assert_eq!(owned.sections()[0].standard.version.value, "1.3");
    assert!(owned.sections()[0]
        .metadata_version
        .as_ref()
        .unwrap()
        .description
        .is_none());
}
#[test]
fn default_text_and_empty_root_work_are_admitted_before_ownership() {
    let mut a = Arena::default();
    let guide = a.guide("SDTMIG");
    let specs = a.node(N::Sequence(vec![]));
    let define = a.text("d.xml");
    let section = a.map(vec![
        ("standard", guide),
        ("specs", specs),
        ("define", define),
    ]);
    let root = a.map(vec![("sdtm", section)]);
    let input = a.finish(root);
    let source_bytes = input
        .nodes()
        .iter()
        .filter_map(|v| {
            if let N::Text(text) = v {
                Some(text.len())
            } else {
                None
            }
        })
        .sum();
    let error = submission_document::decode(
        &input,
        root,
        Limits {
            text_bytes: source_bytes,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.cause, Cause::Limit(Resource::TextBytes));
    let owned = submission_document::decode(
        &input,
        root,
        Limits {
            text_bytes: source_bytes + 2,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(owned.sections()[0].xml_lang.value, "en");
    assert_eq!(owned.sections()[0].xml_lang.origin.node, None);
    let empty = Document::new(vec![N::Mapping(vec![])], 0, DocumentLimits::default()).unwrap();
    assert_eq!(
        submission_document::decode(
            &empty,
            0,
            Limits {
                work: 0,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .cause,
        Cause::Limit(Resource::Work)
    );
}
#[test]
fn control_and_unicode_text_survive_ownership_while_complete_escaping_capacity_is_bounded() {
    let mut a = Arena::default();
    let study = a.text_map(&[
        ("id", "STUDY"),
        ("name", "A\nB\t'\\"),
        ("description", "é < 漢字 & 😀"),
        ("protocol_name", "P"),
    ]);
    let root = a.map(vec![("study", study)]);
    let input = a.finish(root);
    let text_bytes: usize = input
        .nodes()
        .iter()
        .filter_map(|v| {
            if let N::Text(text) = v {
                Some(text.len())
            } else {
                None
            }
        })
        .sum();
    let texts = input
        .nodes()
        .iter()
        .filter(|v| matches!(v, N::Text(_)))
        .count();
    let escaped = text_bytes * 10 + texts * 2;
    let error = submission_document::decode(
        &input,
        root,
        Limits {
            escaped_bytes: escaped - 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.cause, Cause::Limit(Resource::EscapedBytes));
    assert!(std::ptr::eq(error.input, &input));
    let owned = submission_document::decode(
        &input,
        root,
        Limits {
            escaped_bytes: escaped,
            ..Limits::default()
        },
    )
    .unwrap();
    drop(input);
    assert_eq!(owned.study().unwrap().name.value, "A\nB\t'\\");
    assert_eq!(owned.study().unwrap().description.value, "é < 漢字 & 😀");
}

struct DocumentItemFact {
    family: F,
    sequence: usize,
    node: usize,
    index: usize,
}

// Independent normalized occurrences: valid mappings surround two different
// malformed shapes in every family. Expected facts come from this arena alone.
fn malformed_document_lists() -> (Document, Vec<DocumentItemFact>) {
    let mut a = Arena::default();
    let mut sections = Vec::new();
    let mut facts = Vec::new();
    for (name, family, guide) in [
        ("send", F::Send, "SENDIG"),
        ("sdtm", F::Sdtm, "SDTMIG"),
        ("adam", F::Adam, "ADaMIG"),
    ] {
        let guide = a.guide(guide);
        let spec = a.text("specifications/domain.yaml");
        let specs = a.node(N::Sequence(vec![spec]));
        let define = a.text("results/define.xml");
        let first = a.text_map(&[
            ("id", "first"),
            ("kind", "supplemental"),
            ("link", "docs/first.pdf"),
            ("title", "First document"),
        ]);
        let null = a.node(N::Null);
        let scalar = a.node(N::Integer("42".into()));
        let last = a.text_map(&[
            ("id", "last"),
            ("kind", "other"),
            ("link", "docs/last.pdf"),
            ("title", "Last document"),
        ]);
        let sequence = a.node(N::Sequence(vec![first, null, scalar, last]));
        let section = a.map(vec![
            ("documents", sequence),
            ("standard", guide),
            ("specs", specs),
            ("define", define),
        ]);
        sections.push((name, section));
        for (index, node) in [(1, null), (2, scalar)] {
            facts.push(DocumentItemFact {
                family,
                sequence,
                node,
                index,
            });
        }
    }
    let root = a.map(sections);
    (a.finish(root), facts)
}

#[test]
fn malformed_document_items_retain_actual_occurrences_in_every_family_and_position() {
    let (input, expected) = malformed_document_lists();
    let error = submission_document::decode(&input, input.root(), Limits::default()).unwrap_err();
    assert!(std::ptr::eq(error.input, &input));
    let Cause::Findings(findings) = error.cause else {
        panic!("complete shape findings")
    };
    assert_eq!(findings.len(), 6);
    for (finding, fact) in findings.iter().zip(&expected) {
        assert_eq!(finding.kind, Kind::ExpectedMapping);
        assert_eq!(finding.node, fact.node);
        assert_eq!(finding.container, Some(fact.sequence));
        assert_eq!(finding.key, None);
        assert_eq!(finding.requirement(), None);
        assert_eq!(
            finding.origin,
            Some(submission_document::Origin {
                container: fact.sequence,
                key: None,
                node: Some(fact.node),
                field: F::Documents,
            })
        );
        assert_eq!(
            finding.path.steps().collect::<Vec<_>>(),
            [
                Step::Field(fact.family),
                Step::Field(F::Documents),
                Step::Index(fact.index)
            ]
        );
        let N::Sequence(items) = &error.input.nodes()[fact.sequence] else {
            panic!("original document sequence")
        };
        assert_eq!(items[fact.index], fact.node);
        let section = input.field(input.root(), fact.family.name()).unwrap();
        assert_eq!(input.field(section, "documents"), Some(fact.sequence));
        assert_ne!(section, fact.sequence);
    }
    assert!(matches!(input.nodes()[expected[0].node], N::Null));
    assert_eq!(input.nodes()[expected[1].node], N::Integer("42".into()));
}

#[test]
fn document_item_finding_and_path_capacities_admit_the_complete_set_or_keep_raw_input() {
    let (input, _) = malformed_document_lists();
    for (limits, resource) in [
        (
            Limits {
                findings: 5,
                ..Limits::default()
            },
            Resource::Findings,
        ),
        (
            Limits {
                path_segments: 17,
                ..Limits::default()
            },
            Resource::PathSegments,
        ),
    ] {
        let error = submission_document::decode(&input, input.root(), limits).unwrap_err();
        assert!(std::ptr::eq(error.input, &input));
        assert_eq!(error.cause, Cause::Limit(resource));
        assert_eq!(error.input, &input);
    }
    let error = submission_document::decode(
        &input,
        input.root(),
        Limits {
            findings: 6,
            path_segments: 18,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert!(std::ptr::eq(error.input, &input));
    assert!(matches!(error.cause, Cause::Findings(v) if v.len() == 6));
}

#[test]
fn valid_document_fields_and_omitted_defaults_remain_owned_after_original_input_drops() {
    let input = full();
    let adam = input.field(input.root(), "adam").unwrap();
    let sequence = input.field(adam, "documents").unwrap();
    let N::Sequence(items) = &input.nodes()[sequence] else {
        panic!("document list")
    };
    let document_nodes = items.clone();
    let field_nodes = document_nodes
        .iter()
        .map(|&node| ["id", "kind", "link", "title"].map(|name| input.field(node, name).unwrap()))
        .collect::<Vec<_>>();
    let owned = submission_document::decode(&input, input.root(), Limits::default()).unwrap();
    drop(input);
    let docs = &owned.sections()[1].documents;
    assert_eq!(docs.origin.container, adam);
    assert_eq!(docs.origin.node, Some(sequence));
    assert!(docs.origin.key.is_some());
    assert_eq!(docs.value.len(), 3);
    for ((document, &node), expected) in docs.value.iter().zip(&document_nodes).zip(&field_nodes) {
        assert_eq!(document.node, node);
        for (origin, &value_node) in [
            document.id.origin,
            document.kind.origin,
            document.link.origin,
            document.title.origin,
        ]
        .iter()
        .zip(expected)
        {
            assert_eq!(origin.container, node);
            assert_eq!(origin.node, Some(value_node));
            assert!(origin.key.is_some());
        }
    }
    assert_eq!(docs.value[0].title.value, "Reviewer guide");
    assert_eq!(docs.value[1].kind.value, DocumentKind::AnnotatedCrf);
    assert_eq!(docs.value[2].link.value, "notes/data.txt");
    let send = &owned.sections()[0];
    assert!(send.documents.value.is_empty());
    assert_eq!(send.documents.origin.container, send.origin.node.unwrap());
    assert_eq!(send.documents.origin.key, None);
    assert_eq!(send.documents.origin.node, None);
}

#[test]
fn approved_source_system_version_is_admitted_for_every_section() {
    for (family, guide) in [("sdtm", "SDTMIG"), ("adam", "ADaMIG"), ("send", "SENDIG")] {
        let mut a = Arena::default();
        a.section(guide);
        let N::Mapping(mut fields) = a.0.pop().unwrap() else {
            unreachable!()
        };
        let key = a.text("source_system_version");
        let version = a.text("1.0");
        fields.push((key, version));
        let section = a.node(N::Mapping(fields));
        let root = a.map(vec![(family, section)]);
        let input = a.finish(root);
        let owned = submission_document::decode(&input, root, Limits::default()).unwrap();
        drop(input);
        let section = &owned.sections()[0];
        let field = section.source_system_version.as_ref().unwrap();
        assert_eq!(field.value, "1.0");
        assert_eq!(field.origin.field, F::SourceSystemVersion);
        assert_eq!(field.origin.node, Some(version));
        assert_eq!(field.origin.key, Some(key));
        let selected = section.source_versions("0.3.0").unwrap();
        assert_eq!(selected.metadata(), "1.0");
        assert_eq!(selected.installed(), "0.3.0");
        assert_eq!(selected.override_origin(), Some(field.origin));
        assert_eq!(selected.metadata().as_ptr(), field.value.as_ptr());
        assert_eq!(
            section.source_versions(""),
            Err(submission_document::VersionError::EmptyInstalledVersion)
        );
    }
}

#[test]
fn omitted_submission_version_uses_actual_generator_without_creating_an_authored_field() {
    let input = full();
    let owned = submission_document::decode(&input, input.root(), Limits::default()).unwrap();
    for section in owned.sections() {
        assert!(section.source_system_version.is_none());
        for installed in ["0.3.0", "0.4.0.dev7"] {
            let selected = section.source_versions(installed).unwrap();
            assert_eq!(selected.metadata(), installed);
            assert_eq!(selected.installed(), installed);
            assert_eq!(selected.override_origin(), None);
            assert_eq!(selected.installed().as_ptr(), installed.as_ptr());
        }
    }
}

#[test]
fn submission_version_refusals_preserve_exact_nodes_and_do_not_choose_defaults() {
    for (value, expected) in [
        (N::Null, Kind::ExpectedText),
        (N::Boolean(true), Kind::ExpectedText),
        (N::Integer("1".into()), Kind::ExpectedText),
        (N::Text("".into()), Kind::EmptyText),
    ] {
        let mut a = Arena::default();
        a.section("ADaMIG");
        let N::Mapping(mut fields) = a.0.pop().unwrap() else {
            unreachable!()
        };
        let key = a.text("source_system_version");
        let version = a.node(value);
        fields.push((key, version));
        let section = a.node(N::Mapping(fields));
        let root = a.map(vec![("adam", section)]);
        let input = a.finish(root);
        let error = submission_document::decode(&input, root, Limits::default()).unwrap_err();
        assert!(std::ptr::eq(error.input, &input));
        let Cause::Findings(findings) = error.cause else {
            panic!("findings")
        };
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, expected);
        assert_eq!(findings[0].node, version);
        assert_eq!(findings[0].key, Some(key));
        assert_eq!(
            findings[0].path.steps().collect::<Vec<_>>(),
            [Step::Field(F::Adam), Step::Field(F::SourceSystemVersion)]
        );
        assert_eq!(findings[0].requirement(), None);
    }
}

#[test]
fn submission_version_text_is_admitted_before_ownership_under_complete_quotas() {
    let mut a = Arena::default();
    a.section("SENDIG");
    let N::Mapping(mut fields) = a.0.pop().unwrap() else {
        unreachable!()
    };
    let key = a.text("source_system_version");
    let version = a.text(&"v".repeat(4096));
    fields.push((key, version));
    let section = a.node(N::Mapping(fields));
    let root = a.map(vec![("send", section)]);
    let input = a.finish(root);
    for (limits, resource) in [
        (
            Limits {
                text_bytes: 4095,
                ..Limits::default()
            },
            Resource::TextBytes,
        ),
        (
            Limits {
                escaped_bytes: 4095,
                ..Limits::default()
            },
            Resource::EscapedBytes,
        ),
        (
            Limits {
                work: 4095,
                ..Limits::default()
            },
            Resource::Work,
        ),
    ] {
        let error = submission_document::decode(&input, root, limits).unwrap_err();
        assert!(std::ptr::eq(error.input, &input));
        assert_eq!(error.cause, Cause::Limit(resource));
        assert_eq!(input.field(section, "source_system_version"), Some(version));
    }
    let owned = submission_document::decode(&input, root, Limits::default()).unwrap();
    assert_eq!(
        owned.sections()[0]
            .source_system_version
            .as_ref()
            .unwrap()
            .value
            .len(),
        4096
    );
}
