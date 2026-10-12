use std::collections::BTreeSet;
use yamaa_core::{
    diagnostic::{ScalarSubmissionRule as Rule, SCALAR_SUBMISSION_RULES},
    schema::{Document, DocumentLimits, DocumentNode as N},
    specification_submission_document as spec, submission_document as environment,
};

// Authored invalid scalar occurrences, independent of the decoder and schema.
#[derive(Default)]
struct Arena(Vec<N>);
impl Arena {
    fn node(&mut self, value: N) -> usize {
        let node = self.0.len();
        self.0.push(value);
        node
    }
    fn text(&mut self, value: &str) -> usize {
        self.node(N::Text(value.into()))
    }
    fn map(&mut self, fields: &[(&str, usize)]) -> usize {
        let pairs = fields
            .iter()
            .map(|&(key, value)| (self.text(key), value))
            .collect();
        self.node(N::Mapping(pairs))
    }
    fn texts(&mut self, fields: &[(&str, &str)]) -> usize {
        let fields: Vec<_> = fields
            .iter()
            .map(|&(key, value)| (key, self.text(value)))
            .collect();
        self.map(&fields)
    }
    fn finish(self, root: usize) -> Document {
        Document::new(self.0, root, DocumentLimits::default()).unwrap()
    }
}

#[test]
fn actual_raw_decoder_findings_reach_every_scalar_owner_and_keep_shape_uncited() {
    let mut reached = BTreeSet::new();
    let mut a = Arena::default();
    let study = a.texts(&[
        ("id", "9 bad"),
        ("name", "Study"),
        ("description", "Description"),
        ("protocol_name", "P"),
    ]);
    let guide = a.texts(&[("name", "UNKNOWN"), ("version", "3.4"), ("status", "Final")]);
    let specs = a.node(N::Sequence(vec![]));
    let define = a.node(N::Null);
    let doc = a.texts(&[
        ("id", "bad.id"),
        ("kind", "wrong"),
        ("link", "docs/../escape.pdf"),
        ("title", "Document"),
    ]);
    let documents = a.node(N::Sequence(vec![doc]));
    let language = a.text("e-123456789");
    let section = a.map(&[
        ("standard", guide),
        ("specs", specs),
        ("define", define),
        ("documents", documents),
        ("xml_lang", language),
    ]);
    let root = a.map(&[("study", study), ("sdtm", section)]);
    let input = a.finish(root);
    let error = environment::decode(&input, root, environment::Limits::default()).unwrap_err();
    assert!(std::ptr::eq(error.input, &input));
    let environment::Cause::Findings(findings) = error.cause else {
        panic!("raw findings")
    };
    assert_eq!(
        findings
            .iter()
            .map(|finding| (finding.scalar_rule(), finding.requirement()))
            .collect::<Vec<_>>(),
        [
            (Some(Rule::DefineId), Some("REQ-1068")),
            (Some(Rule::StandardName), Some("REQ-1074")),
            (None, None),
            (Some(Rule::Identifier), Some("REQ-1046")),
            (None, None),
            (Some(Rule::Href), Some("REQ-1070")),
            (Some(Rule::Language), Some("REQ-1073")),
        ]
    );
    assert_eq!(findings[2].node, define);
    assert_eq!(findings[2].kind, environment::Kind::ExpectedText);
    assert!(matches!(input.nodes()[findings[2].node], N::Null));
    assert_eq!(findings[4].kind, environment::Kind::InvalidChoice);
    assert_eq!(findings[4].origin.unwrap().container, doc);
    assert_eq!(
        findings[4].path.steps().collect::<Vec<_>>(),
        [
            environment::Step::Field(environment::FieldName::Sdtm),
            environment::Step::Field(environment::FieldName::Documents),
            environment::Step::Index(0),
            environment::Step::Field(environment::FieldName::Kind),
        ]
    );
    reached.extend(findings.iter().filter_map(|finding| finding.scalar_rule()));

    let mut a = Arena::default();
    let label = a.text("Dataset");
    let class = a.text("wrong");
    let subclass = a.text("wrong");
    let structure = a.text("One record per subject");
    let repeating = a.node(N::Boolean(false));
    let dataset = a.map(&[
        ("label", label),
        ("class", class),
        ("subclass", subclass),
        ("structure", structure),
        ("repeating", repeating),
    ]);
    let origin = a.texts(&[("type", "wrong"), ("source", "wrong")]);
    let method = a.texts(&[("type", "wrong"), ("description", "Derive")]);
    let core = a.text("wrong");
    let data_type = a.text("wrong");
    let submission = a.map(&[
        ("core", core),
        ("data_type", data_type),
        ("origin", origin),
        ("method", method),
    ]);
    let name = a.text("bad-name");
    let kind = a.text("str");
    let column = a.map(&[("name", name), ("type", kind), ("submission", submission)]);
    let null = a.node(N::Null);
    let null_submission = a.map(&[("core", null)]);
    let valid_name = a.text("B");
    let valid_kind = a.text("str");
    let second = a.map(&[
        ("name", valid_name),
        ("type", valid_kind),
        ("submission", null_submission),
    ]);
    let columns = a.node(N::Sequence(vec![column, second]));
    let root = a.map(&[("submission", dataset), ("columns", columns)]);
    let input = a.finish(root);
    let error = spec::decode(&input, root, spec::Limits::default()).unwrap_err();
    assert!(std::ptr::eq(error.input, &input));
    let spec::Cause::Findings(findings) = error.cause else {
        panic!("raw findings")
    };
    assert_eq!(
        findings
            .iter()
            .map(|finding| (finding.scalar_rule(), finding.requirement()))
            .collect::<Vec<_>>(),
        [
            (Some(Rule::DatasetClass), Some("REQ-1146")),
            (Some(Rule::DatasetSubclass), Some("REQ-1147")),
            (Some(Rule::Identifier), Some("REQ-1046")),
            (Some(Rule::Core), Some("REQ-1141")),
            (Some(Rule::DataType), Some("REQ-1145")),
            (Some(Rule::OriginType), Some("REQ-1142")),
            (Some(Rule::OriginSource), Some("REQ-1143")),
            (Some(Rule::MethodType), Some("REQ-1144")),
            (None, None),
        ]
    );
    let raw = &findings[8];
    assert_eq!(raw.kind, spec::Kind::ExpectedText);
    assert_eq!(raw.node, null);
    assert_eq!(raw.container, Some(null_submission));
    assert_eq!(raw.origin.unwrap().field, spec::FieldName::Core);
    assert_eq!(
        raw.path.steps().collect::<Vec<_>>(),
        [
            spec::Step::Field(spec::FieldName::Columns),
            spec::Step::Index(1),
            spec::Step::Field(spec::FieldName::Submission),
            spec::Step::Field(spec::FieldName::Core),
        ]
    );
    reached.extend(findings.iter().filter_map(|finding| finding.scalar_rule()));
    assert_eq!(reached, SCALAR_SUBMISSION_RULES.iter().copied().collect());
}
