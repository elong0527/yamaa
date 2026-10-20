//! The file rules' conditions come from the kind a resolver reports
//! (FILE-7, FILE-8; #2845 question 5). A memory resolver models a link, a
//! junction, another reparse point, a directory and any other entry, and the
//! engine refuses each exactly as it refuses what the local file resolver
//! reports, without a filesystem call of its own.
#![cfg(feature = "yaml")]

use std::sync::Arc;

use arrow_array::{Array, ListArray, StringArray};
use yamaa::{
    CancellationToken, Engine, EngineOptions, HostLanguage, MemoryResolver, ResolverFailure,
    ResourceKind, Table,
};

const SPEC: &[u8] = b"schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: input/src.csv, types: {ID: int}}}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";

/// The condition, specification paths and context of each issue.
fn issues(table: &Table) -> Vec<(String, Vec<String>, String)> {
    let mut rows = Vec::new();
    for batch in table.batches() {
        let text = |name: &str| {
            batch
                .column_by_name(name)
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .clone()
        };
        let (condition, context) = (text("condition"), text("context"));
        let paths = batch
            .column_by_name("spec_paths")
            .unwrap()
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap()
            .clone();
        for row in 0..batch.num_rows() {
            let values = paths.value(row);
            let values = values.as_any().downcast_ref::<StringArray>().unwrap();
            rows.push((
                condition.value(row).to_owned(),
                (0..values.len())
                    .map(|i| values.value(i).to_owned())
                    .collect(),
                context.value(row).to_owned(),
            ));
        }
    }
    rows
}

fn engine(resolver: &Arc<MemoryResolver>) -> Engine {
    Engine::new(
        EngineOptions::new(HostLanguage::Python, "kind-test".into(), "0".into())
            .with_resolver(Arc::clone(resolver) as Arc<dyn yamaa::ResourceResolver>),
    )
}

#[test]
fn every_reported_kind_raises_its_file_rule_condition() {
    for (kind, condition) in [
        (ResourceKind::SymbolicLink, "resource_path_symlink"),
        (ResourceKind::Junction, "resource_path_symlink"),
        (ResourceKind::ReparsePoint, "resource_path_symlink"),
        (ResourceKind::Directory, "resource_path_not_regular_file"),
        (ResourceKind::Other, "resource_path_not_regular_file"),
        (ResourceKind::File, "resource_path_not_regular_file"),
        (ResourceKind::Missing, "resource_path_missing"),
    ] {
        let resolver = Arc::new(MemoryResolver::new());
        resolver.insert("/", "spec.yaml", "spec:1", Arc::from(SPEC));
        resolver.insert_kind("/spec.yaml", "input/src.csv", kind);
        let token = CancellationToken::new();
        let run = engine(&resolver)
            .spec_run("spec.yaml", None, &token)
            .unwrap();
        assert!(run.output().is_none(), "{kind:?}");
        let found = issues(&run.issues());
        assert_eq!(found.len(), 1, "{kind:?}: {found:?}");
        let (found_condition, paths, context) = &found[0];
        assert_eq!(found_condition, condition, "{kind:?}");
        assert_eq!(paths, &["input.SRC.path"], "{kind:?}");
        assert!(
            context.contains(r#""path":"input/src.csv""#),
            "{kind:?}: {context}"
        );
        // A check reads no study data, so it never asks about the input.
        assert_eq!(
            engine(&resolver)
                .spec_check("spec.yaml", None, &token)
                .unwrap()
                .issues()
                .rows(),
            0,
            "{kind:?}"
        );
    }
}

/// A caller's resolver reports a kind with [`ResolverFailure::from_kind`],
/// and the engine reads it as the memory resolver's report.
#[test]
fn a_caller_resolver_reports_kinds_through_its_failures() {
    struct Linked(MemoryResolver);
    impl yamaa::ResourceResolver for Linked {
        fn request(
            &self,
            request: yamaa::ResolverRequest<'_>,
        ) -> Result<yamaa::ResolverReply, ResolverFailure> {
            if request.written == "input/src.csv" {
                return Err(ResolverFailure::from_kind(ResourceKind::Junction));
            }
            self.0.request(request)
        }
    }
    let inner = MemoryResolver::new();
    inner.insert("/", "spec.yaml", "spec:1", Arc::from(SPEC));
    let engine = Engine::new(
        EngineOptions::new(HostLanguage::Python, "kind-test".into(), "0".into())
            .with_resolver(Arc::new(Linked(inner))),
    );
    let run = engine
        .spec_run("spec.yaml", None, &CancellationToken::new())
        .unwrap();
    let found = issues(&run.issues());
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "resource_path_symlink");
}
