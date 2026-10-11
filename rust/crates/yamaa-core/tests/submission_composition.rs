//! Independent metadata arenas exercise complete multi-domain work admission.
use yamaa_core::{
    project_terminology::Catalogue,
    schema::{Document, DocumentLimits, DocumentNode as N},
    submission_composition::{self, Input},
    submission_document,
};

#[derive(Default)]
struct Arena(Vec<N>);
impl Arena {
    fn node(&mut self, node: N) -> usize {
        let index = self.0.len();
        self.0.push(node);
        index
    }
    fn text(&mut self, value: &str) -> usize {
        self.node(N::Text(value.into()))
    }
    fn map(&mut self, fields: Vec<(&str, usize)>) -> usize {
        let entries = fields
            .into_iter()
            .map(|(name, value)| (self.text(name), value))
            .collect();
        self.node(N::Mapping(entries))
    }
    fn texts(&mut self, fields: &[(&str, &str)]) -> usize {
        let entries = fields
            .iter()
            .map(|&(name, value)| (name, self.text(value)))
            .collect();
        self.map(entries)
    }
    fn finish(self, root: usize) -> Document {
        Document::new(self.0, root, DocumentLimits::default()).unwrap()
    }
}
fn environment(count: usize) -> Document {
    let mut a = Arena::default();
    let study = a.texts(&[
        ("id", "STUDY01"),
        ("name", "Independent admission study"),
        ("description", "Thirty domains with column and row metadata"),
        ("protocol_name", "PROTOCOL01"),
    ]);
    let standard = a.texts(&[("name", "SDTMIG"), ("version", "3.4"), ("status", "Final")]);
    let entries = (0..count)
        .map(|index| a.text(&format!("d{index}.yaml")))
        .collect();
    let specs = a.node(N::Sequence(entries));
    let define = a.text("define.xml");
    let section = a.map(vec![
        ("standard", standard),
        ("specs", specs),
        ("define", define),
    ]);
    let root = a.map(vec![("study", study), ("sdtm", section)]);
    a.finish(root)
}
fn specification(index: usize, count: usize, row_count: usize) -> Document {
    let mut a = Arena::default();
    let domain = a.text(&format!("D{index}"));
    let label = a.text("Independent dataset");
    let class = a.text("SPECIAL PURPOSE");
    let structure = a.text("One record per subject");
    let repeating = a.node(N::Boolean(false));
    let dataset = a.map(vec![
        ("label", label),
        ("class", class),
        ("structure", structure),
        ("repeating", repeating),
    ]);
    let mut columns = Vec::new();
    let mut names = Vec::new();
    for column in 0..count {
        let name = format!("V{column:04}");
        names.push(a.text(&name));
        let name = a.text(&name);
        let kind = a.text("str");
        let label = a.text("Independent column");
        let derivation = a.texts(&[("literal", "X")]);
        let origin = a.texts(&[("type", "Assigned"), ("source", "Sponsor")]);
        let core = a.text("Exp");
        let length = a.node(N::Integer("1".into()));
        let submission = a.map(vec![("core", core), ("length", length), ("origin", origin)]);
        columns.push(a.map(vec![
            ("name", name),
            ("type", kind),
            ("label", label),
            ("derivation", derivation),
            ("submission", submission),
        ]));
    }
    let columns = a.node(N::Sequence(columns));
    let names = a.node(N::Sequence(names));
    let path = a.text(&format!("d{index}.csv"));
    let output = a.map(vec![("path", path), ("columns", names)]);
    let mut rows = Vec::new();
    for _ in 0..row_count {
        let entries = (0..count)
            .map(|column| {
                let name = a.text(&format!("V{column:04}"));
                let value = a.texts(&[("literal", "X")]);
                (name, value)
            })
            .collect();
        let derivations = a.node(N::Mapping(entries));
        rows.push(a.map(vec![("derivations", derivations)]));
    }
    let rows = a.node(N::Sequence(rows));
    let root = a.map(vec![
        ("domain", domain),
        ("submission", dataset),
        ("output", output),
        ("columns", columns),
        ("rows", rows),
    ]);
    a.finish(root)
}
fn compose(
    documents: &[Document],
) -> Result<submission_composition::Study, Vec<submission_composition::Finding>> {
    let environment = environment(documents.len());
    let declared =
        submission_document::decode(&environment, environment.root(), Default::default()).unwrap();
    let sources: Vec<_> = (0..documents.len())
        .map(|index| format!("d{index}.yaml"))
        .collect();
    let artifacts: Vec<_> = (0..documents.len())
        .map(|index| format!("d{index}.csv"))
        .collect();
    let inputs: Vec<_> = documents
        .iter()
        .enumerate()
        .map(|(index, document)| Input {
            document,
            source: &sources[index],
            artifact: &artifacts[index],
            href: &artifacts[index],
        })
        .collect();
    submission_composition::compose(
        declared.study().unwrap(),
        &declared.sections()[0],
        &Catalogue::admit(vec![]).unwrap(),
        &inputs,
        "1.0",
        "2026-09-11T00:00:00",
    )
}
#[test]
fn thirty_domains_with_fifteen_hundred_nodes_admit_complete_column_and_row_scans() {
    let documents: Vec<_> = (0..30).map(|index| specification(index, 30, 6)).collect();
    assert!(documents
        .iter()
        .all(|document| (1400..1800).contains(&document.nodes().len())));
    let study = compose(&documents).unwrap();
    assert_eq!(study.datasets().len(), 30);
    for (index, dataset) in study.datasets().iter().enumerate() {
        assert_eq!(dataset.id(), format!("D{index}"));
        assert_eq!(dataset.columns().len(), 30);
        assert_eq!(dataset.columns()[29].declaration().name.value, "V0029");
    }
}
#[test]
fn excessive_selected_column_scans_refuse_the_complete_section() {
    // These arenas fit the cumulative node/text ownership limits, but repeating
    // complete row/name scans for every selected column exceeds the work bound.
    let documents: Vec<_> = (0..30).map(|index| specification(index, 250, 0)).collect();
    assert!(
        documents
            .iter()
            .map(|document| document.nodes().len())
            .sum::<usize>()
            < 524_288
    );
    let findings = compose(&documents).unwrap_err();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].definition.condition, "engine_rejected");
    assert!(format!("{:?}", findings[0].context).contains("composition_admission_limit"));
}
