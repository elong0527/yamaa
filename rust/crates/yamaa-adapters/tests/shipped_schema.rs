//! Shipped schema identity and raw preparation without host schema interpretation.
use yamaa_adapters::{
    shipped_schema,
    specification_source::{Error, InheritanceError, InheritancePort, Source},
};
use yamaa_core::schema::NormalizationError;
use yamaa_engine::inheritance::{Source as Identity, SourceError};

struct NoParents;
impl InheritancePort for NoParents {
    type Error = ();
    fn canonicalize(&mut self, _: &str, _: &str) -> Result<Identity, SourceError<()>> {
        panic!("entry rejection or standalone preparation must not resolve a parent")
    }
    fn capture(&mut self, _: &Identity, _: usize) -> Result<Vec<u8>, SourceError<()>> {
        panic!("entry rejection or standalone preparation must not read a parent")
    }
    fn rebase(&mut self, _: &Identity, _: &Identity, _: &str, _: usize) -> Result<String, ()> {
        panic!("entry rejection or standalone preparation must not rebase a parent")
    }
}
fn source(text: &str) -> Source {
    Source {
        identity: "arbitrary/location/domain.yaml".into(),
        bytes: text.as_bytes().to_vec(),
    }
}
const VALID: &str = "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: input.csv}\noutput: {path: output.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: {source: SRC.ID}}\n";

#[test]
fn shipped_closure_retains_every_authoritative_byte_and_admits_without_a_directory() {
    let schema = shipped_schema::capture().unwrap();
    assert_eq!(schema.structure().version(), "1.0");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../yaml");
    let mut expected = std::fs::read_dir(&root)
        .unwrap()
        .map(|p| p.unwrap().file_name().into_string().unwrap())
        .filter(|p| p == "schema.yaml" || p.starts_with("schema_") && p.ends_with(".yaml"))
        .filter(|p| p != "schema_define.yaml" && p != "schema_environment.yaml")
        .collect::<Vec<_>>();
    expected.sort();
    let mut actual = schema
        .sources()
        .iter()
        .map(|s| s.identity.clone())
        .collect::<Vec<_>>();
    actual.sort();
    assert_eq!(actual, expected);
    for module in schema.sources() {
        assert_eq!(
            module.bytes,
            std::fs::read(root.join(&module.identity)).unwrap()
        );
    }
    let prepared =
        shipped_schema::prepare(source(VALID), "domain.yaml".into(), &mut NoParents).unwrap();
    assert!(prepared.inheritance().is_none());
    assert_eq!(prepared.source().bytes, VALID.as_bytes());
    assert!(prepared.parents().is_empty());
}

#[test]
fn unsupported_or_missing_version_is_rejected_before_any_parent_authority() {
    for text in [
        VALID.replace("'1.0'", "'99.0'"),
        VALID.replace("schema_version: '1.0'\n", ""),
    ] {
        let result = shipped_schema::prepare(source(&text), "domain.yaml".into(), &mut NoParents);
        let Err(InheritanceError::Entry(Error::Normalize(NormalizationError::Invalid(findings)))) =
            result
        else {
            panic!("expected shared schema version finding");
        };
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].condition, "schema_version_mismatch");
    }
    let malformed =
        shipped_schema::prepare(source("[unclosed"), "domain.yaml".into(), &mut NoParents);
    assert!(matches!(
        malformed,
        Err(InheritanceError::Entry(Error::Decode { .. }))
    ));
    // Presence selects inheritance even for an invalid null field; no fallback.
    let inherited = shipped_schema::prepare(
        source(&(VALID.to_owned() + "parents: null\n")),
        "domain.yaml".into(),
        &mut NoParents,
    );
    assert!(matches!(
        inherited,
        Err(InheritanceError::Preparation { .. })
    ));
}
