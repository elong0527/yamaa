//! Host resources: held bytes reused across aliases, absent and cancelled
//! resources, frame freshness, staged output that an interruption or abort
//! leaves behind, and parent files on another drive, on another share, linked
//! or malformed.
use std::sync::Arc;
#[cfg(feature = "yaml")]
use yamaa_adapters::host_resources::{Operation, Reply, Request, Resolver};
use yamaa_adapters::{
    column_table::{self, ColumnTable},
    file_resources::{Error, Resources},
    host_resources::{Failure, MemoryResolver},
};
use yamaa_core::{
    table::{Column, TableSchema},
    temporal::{Date, DateTime, Precision},
    value::ColumnType,
};

/// Seam: the bytes `Resources` holds for one reference, reused by pointer
/// across its aliases and counted by its capture counter, and its typed
/// refusal of a changed object, through an in-memory resolver.
#[test]
fn explicit_reference_reuses_held_bytes_across_aliases_and_refuses_changes() {
    let backend = Arc::new(MemoryResolver::default());
    backend.insert(
        "/",
        "one.csv",
        "object:version-1",
        Arc::from(&b"A\n7\n"[..]),
    );
    backend.insert(
        "/",
        "alias.csv",
        "object:version-1",
        Arc::from(&b"A\n7\n"[..]),
    );
    let mut resources = Resources::with_resolver(backend.clone(), "spec.yaml");
    let (one, fresh) = resources.capture("one.csv").unwrap();
    assert!(fresh);
    let (alias, fresh) = resources.capture("alias.csv").unwrap();
    assert!(!fresh);
    assert!(Arc::ptr_eq(&one, &alias));
    assert_eq!(resources.capture_reads(), 1);
    backend.insert(
        "/",
        "one.csv",
        "object:version-2",
        Arc::from(&b"A\n7\n"[..]),
    );
    assert_eq!(resources.capture("one.csv"), Err(Error::Changed));
    assert_eq!(one.as_ref(), b"A\n7\n");
    let mut fresh_attempt = Resources::with_resolver(backend, "spec.yaml");
    assert!(fresh_attempt.capture("one.csv").unwrap().1);
}

/// Seam: a cancelled in-memory resolver, the resources' typed `Error` and
/// their capture counter, which no issue row states.
#[test]
fn absent_and_cancelled_resources_remain_explicit_without_captures() {
    let backend = Arc::new(MemoryResolver::default());
    let resources = Resources::with_resolver(backend.clone(), "spec.yaml");
    assert_eq!(resources.inspect("missing.csv"), Err(Error::Missing));
    backend.cancel();
    assert_eq!(
        resources.inspect("missing.csv"),
        Err(Error::Host(Failure::Cancelled))
    );
    assert_eq!(resources.capture_reads(), 0);
}

fn frame(
    float: f64,
    date_precision: Precision,
    timestamp_precision: Precision,
) -> Arc<ColumnTable> {
    let date = Date::new(2026, 1, 1, date_precision).unwrap();
    let timestamp = DateTime::new(date, 0, 0, 0, timestamp_precision).unwrap();
    let logical = TableSchema::new(vec![
        Column {
            name: "F".into(),
            kind: ColumnType::Float,
        },
        Column {
            name: "D".into(),
            kind: ColumnType::Date,
        },
        Column {
            name: "T".into(),
            kind: ColumnType::DateTime,
        },
    ])
    .unwrap();
    let columns = vec![
        column_table::Column::floats([Some(float)]),
        column_table::Column::Date(vec![Some(date)]),
        column_table::Column::DateTime(vec![Some(timestamp)]),
    ];
    Arc::new(ColumnTable::new(logical, columns, 1).unwrap())
}

/// Seam: the freshness of in-memory frames served under one reference
/// (-0.0 against 0.0, and collected temporal precision), with no file a case
/// could stage.
#[test]
fn frame_freshness_compares_signed_zero_and_collected_temporal_precision() {
    for changed in [
        frame(-0.0, Precision::Day, Precision::Second),
        frame(0.0, Precision::Year, Precision::Second),
        frame(0.0, Precision::Day, Precision::Day),
    ] {
        let backend = Arc::new(MemoryResolver::default());
        let original = frame(0.0, Precision::Day, Precision::Second);
        backend.insert_frame("/", "raw.csv", "warehouse:version-1", original);
        let mut resources = Resources::with_resolver(backend.clone(), "spec.yaml");
        assert!(resources.capture("raw.csv").unwrap().1);
        backend.insert_frame("/", "raw.csv", "warehouse:version-1", changed);
        assert_eq!(resources.capture("raw.csv"), Err(Error::Changed));
    }
}

#[cfg(feature = "yaml")]
struct InterruptStage(MemoryResolver);
#[cfg(feature = "yaml")]
impl Resolver for InterruptStage {
    fn request(&self, request: Request<'_>) -> Result<Reply, Failure> {
        // Stage is atomic: a failed Stage leaves no provisional output, so
        // fail before delegating instead of staging then reporting failure.
        if request.operation == Operation::Stage {
            return Err(Failure::Cancelled);
        }
        let operation = request.operation;
        let reply = self.0.request(request)?;
        if operation == Operation::Stage {
            Err(Failure::Cancelled)
        } else {
            Ok(reply)
        }
    }
}

/// Seam: a caller's resolver scripted to cancel its Stage operation, and
/// the staged and published objects it holds afterwards.
#[cfg(feature = "yaml")]
#[test]
fn interrupted_stage_leaves_no_provisional_output() {
    let backend = Arc::new(InterruptStage(MemoryResolver::default()));
    let spec = b"yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {RAW: raw.csv}\ncolumns: [{name: ID, type: int, derivation: RAW.ID}]\noutput: {path: out.csv, columns: [ID]}\n";
    backend
        .0
        .insert("/", "spec.yaml", "object:spec-1", Arc::from(&spec[..]));
    backend.0.insert(
        "/spec.yaml",
        "raw.csv",
        "object:data-1",
        Arc::from(&b"ID\n7\n"[..]),
    );
    let resources = Resources::with_resolver(backend.clone(), "spec.yaml");
    let prepared =
        yamaa_adapters::file_preparation::FileSpecification::prepare(resources, "spec.yaml")
            .unwrap();
    let mut result = yamaa_adapters::file_application::run(
        &prepared,
        &mut prepared.run_scope(None),
        &mut crate::support::ports::NoFunctions,
        |_| yamaa_adapters::file_application::Classification::Opaque,
        yamaa_adapters::specification_report::Identity {
            runtime: "rust",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "domain",
            specification: "spec.yaml",
            base_directory: ".",
        },
        yamaa_core::application_observer::Scope::default(),
    )
    .1
    .unwrap();
    assert!(!result.save().unwrap());
    assert_eq!(backend.0.staged_count(), 0);
    assert!(backend.0.published("memory:/out.csv").is_none());
}

/// Seam: the Stage and Abort protocol of the in-memory resolver, asked
/// directly, and the staged objects it holds.
#[cfg(feature = "yaml")]
#[test]
fn abort_removes_only_the_staging_token_not_the_target() {
    use yamaa_adapters::host_resources::{Kind, Operation};
    let backend = MemoryResolver::default();
    let stage = |bytes: &[u8]| {
        backend.request(Request {
            operation: Operation::Stage,
            kind: Kind::Output,
            declaring: "/spec.yaml",
            written: "out.csv",
            reference: Some("memory:/out.csv"),
            bytes: Some(bytes),
            limit: None,
        })
    };
    let token = stage(b"ID\n7\n").unwrap().reference;
    let second = stage(b"ID\n8\n").unwrap().reference;
    assert_ne!(token, second);
    assert_eq!(backend.staged_count(), 2);
    // Abort by the target identity must not delete another attempt's
    // provisional output; only the staging token cleans up.
    backend
        .request(Request {
            operation: Operation::Abort,
            kind: Kind::Output,
            declaring: "/spec.yaml",
            written: "out.csv",
            reference: Some("memory:/out.csv"),
            bytes: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(backend.staged_count(), 2);
    backend
        .request(Request {
            operation: Operation::Abort,
            kind: Kind::Output,
            declaring: "/spec.yaml",
            written: "out.csv",
            reference: Some(&token),
            bytes: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(backend.staged_count(), 1);
    backend
        .request(Request {
            operation: Operation::Abort,
            kind: Kind::Output,
            declaring: "/spec.yaml",
            written: "out.csv",
            reference: Some(&second),
            bytes: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(backend.staged_count(), 0);
}

/// A declaring identity and a written path.
#[cfg(feature = "yaml")]
type Name = (String, String);

/// Serves each name under the location a Windows walk would give it, so a
/// test on any host can place an entry on a local drive and its parent on
/// another drive or on a network share, and records every capture.
#[cfg(feature = "yaml")]
#[derive(Default)]
struct Located {
    objects: std::collections::BTreeMap<Name, (String, Arc<[u8]>)>,
    captures: std::sync::Mutex<Vec<Name>>,
}
#[cfg(feature = "yaml")]
impl Located {
    fn insert(&mut self, declaring: &str, written: &str, location: &str, bytes: &[u8]) {
        self.objects.insert(
            (declaring.into(), written.into()),
            (location.into(), Arc::from(bytes)),
        );
    }
}
#[cfg(feature = "yaml")]
impl Resolver for Located {
    fn request(&self, request: Request<'_>) -> Result<Reply, Failure> {
        if !matches!(request.operation, Operation::Inspect | Operation::Capture) {
            return Err(Failure::Rejected);
        }
        let (location, bytes) = self
            .objects
            .get(&(request.declaring.to_owned(), request.written.to_owned()))
            .ok_or(Failure::Missing)?;
        if request.reference.is_some_and(|held| held != location) {
            return Err(Failure::Changed);
        }
        let capture = request.operation == Operation::Capture;
        if capture {
            self.captures
                .lock()
                .unwrap()
                .push((request.declaring.into(), request.written.into()));
        }
        Ok(Reply {
            bytes: capture.then(|| Arc::clone(bytes)),
            location: Some(location.clone()),
            ..Reply::reference(location.clone())
        })
    }
}

/// An entry at `entry` whose parent, written `P:/std/base.yaml`, lies at
/// `parent`: the parent declares its input beside itself (FILE-1, LAYER-10).
/// Folders named like the parent's share also hold that input beside the
/// entry, where a parent step through the entry's root would lead.
#[cfg(feature = "yaml")]
fn parent_elsewhere(entry: &str, parent: &str) -> Located {
    const ENTRY: &[u8] = b"yamaa: \"1.0\"\ntype: spec\nparent: P:/std/base.yaml\ndomain: CHILD\n";
    const PARENT: &[u8] = b"yamaa: \"1.0\"\ntype: spec\ndomain: BASE\nkeys: [ID]\ninput: {SRC: {path: input/src.csv, types: {ID: int}}}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
    let mut resolver = Located::default();
    resolver.insert("/", "child.yaml", entry, ENTRY);
    resolver.insert(entry, "P:/std/base.yaml", parent, PARENT);
    for climb in ["..", "../.."] {
        let written = format!("{climb}/server/share/std/input/src.csv");
        resolver.insert(
            entry,
            &written,
            "C:/server/share/std/input/src.csv",
            b"ID\n999\n",
        );
    }
    resolver
}

/// Seam: a resolver serving Windows locations on any host, and the log of
/// the captures it served, which no issue row, table or saved file states.
#[cfg(feature = "yaml")]
#[test]
fn a_parent_on_another_drive_reads_its_input_beside_itself() {
    for entry in ["C:/study/child.yaml", "//server/share/study/child.yaml"] {
        let mut resolver = parent_elsewhere(entry, "D:/std/base.yaml");
        resolver.insert(
            entry,
            "D:/std/input/src.csv",
            "D:/std/input/src.csv",
            b"ID\n7\n",
        );
        let resolver = Arc::new(resolver);
        let resources = Resources::with_resolver(resolver.clone(), "child.yaml");
        let prepared =
            yamaa_adapters::file_preparation::FileSpecification::prepare(resources, "child.yaml")
                .unwrap();
        // No relative path climbs from the entry's root to `D:`, so the
        // location is written whole.
        assert_eq!(prepared.run().source().path, "D:/std/input/src.csv");
        let mut run = prepared.run_scope(None);
        let attempt = prepared.build(
            &mut run,
            &mut crate::support::ports::NoFunctions,
            yamaa_core::application_observer::Scope::default(),
        );
        assert!(attempt.dataset.result.is_ok());
        assert_eq!(
            resolver.captures.lock().unwrap().last(),
            Some(&(entry.to_owned(), "D:/std/input/src.csv".to_owned()))
        );
    }
}

/// Seam: a resolver serving Windows locations on any host, and the log of
/// the captures it served, so the refusal is shown to read nothing from the
/// entry's root.
#[cfg(feature = "yaml")]
#[test]
fn a_parent_on_another_share_is_refused_rather_than_read_from_the_entry_root() {
    use yamaa_core::project_function::Language;
    const PARENT: &str = "//server/share/std/base.yaml";
    for (entry, entry_root) in [
        // The entry on a local drive, the parent on a share it maps.
        ("C:/study/child.yaml", "C:"),
        // The entry below `/`.
        ("/study/child.yaml", "/"),
        // The entry on one share, the parent on another.
        ("//other/share/study/child.yaml", "//other/share"),
    ] {
        // LAYER-10's finding, never the opaque engine refusal: the parent,
        // the path it writes beside itself and both roots.
        let refused = |issues: &[yamaa_core::diagnostic::Diagnostic], route: &str| {
            assert_eq!(issues.len(), 1, "{entry} {route}: {issues:?}");
            let issue = &issues[0];
            assert_eq!(
                (issue.phase(), issue.condition(), issue.rule()),
                ("validation", "parent_on_another_share", Some("LAYER-10")),
                "{entry} {route}"
            );
            assert_eq!(issue.spec_paths, ["parent"], "{entry} {route}");
            assert_eq!(
                yamaa_core::json::Value::Object(issue.context.clone()),
                yamaa_adapters::json!({
                    "source": PARENT,
                    "path": "input/src.csv",
                    "parent_root": "//server/share",
                    "entry_root": entry_root,
                }),
                "{entry} {route}"
            );
        };
        let request = || yamaa_core::build::Request {
            specification: "child.yaml",
            environment: None,
        };
        let resolver = Arc::new(parent_elsewhere(entry, PARENT));
        let Err(error) = yamaa_adapters::file_workflow::select_with_resolver(
            request(),
            Some(Language::Python),
            resolver.clone(),
        ) else {
            panic!("a parent at {PARENT} was rebased onto the root of {entry}");
        };
        refused(
            &yamaa_adapters::project_application::selection_failure(&error),
            "select",
        );
        // Only the two specifications were read: nothing under folders named
        // like the parent's share, in its input's place.
        let captures = resolver.captures.lock().unwrap();
        assert_eq!(
            captures
                .iter()
                .map(|(_, written)| written.as_str())
                .collect::<Vec<_>>(),
            ["child.yaml", "P:/std/base.yaml"],
            "{entry}"
        );
        // A check, a resolve and a direct preparation report the same.
        refused(
            &yamaa_adapters::file_application::check_with_resolver(
                Some(Language::Python),
                request(),
                yamaa_adapters::specification_report::Identity {
                    runtime: "rust",
                    runtime_version: "test",
                    engine_version: yamaa_core::VERSION,
                    example: "domain",
                    specification: "child.yaml",
                    base_directory: ".",
                },
                Some(Arc::new(parent_elsewhere(entry, PARENT))),
            ),
            "check",
        );
        let Err(issues) = yamaa_adapters::resolved_artifact::resolve(
            "child.yaml",
            None,
            Some(Language::Python),
            Some(Arc::new(parent_elsewhere(entry, PARENT))),
        ) else {
            panic!("{entry}: resolved a parent on another share");
        };
        refused(&issues, "resolve");
        let resources =
            Resources::with_resolver(Arc::new(parent_elsewhere(entry, PARENT)), "child.yaml");
        let Err(yamaa_adapters::file_preparation::Error::PreparationFinding { diagnostic, .. }) =
            yamaa_adapters::file_preparation::FileSpecification::prepare(resources, "child.yaml")
        else {
            panic!("{entry}: the refusal lost its finding");
        };
        refused(&[*diagnostic], "prepare");
    }
}

/// Records the written path of every request, then serves it from `files`.
#[cfg(feature = "yaml")]
struct Asked<R> {
    files: R,
    written: std::sync::Mutex<Vec<String>>,
}
#[cfg(feature = "yaml")]
impl<R: Resolver> Resolver for Asked<R> {
    fn request(&self, request: Request<'_>) -> Result<Reply, Failure> {
        self.written.lock().unwrap().push(request.written.into());
        self.files.request(request)
    }
}

/// Seam: the routes no operation reads, the local resolved artifact and file
/// preparation and every route through a caller's resolver, with the names
/// that resolver was asked for; the local file route's check and build are
/// `fixtures/diagnostics/local_paths.json`'s `a_parent_..._is_refused_as_written`
/// cases. FILE-5 and LAYER-2: a `parent` spelled with an empty segment, a
/// trailing separator, a network share's root or a backslash fails validation
/// with the written-form condition at `parent`, naming the path as written
/// and the layer that writes it, never with the engine's own refusal. The
/// spelling is judged before any resolver is asked for the parent: the local
/// file resolver or a caller's, which here would serve the name.
#[cfg(feature = "yaml")]
#[test]
fn a_malformed_parent_spelling_is_refused_as_written_on_every_route() {
    use yamaa_adapters::file_preparation::{Error as Prepared, FileSpecification};
    use yamaa_core::project_function::Language;
    const PARENT: &[u8] = b"yamaa: '1.0'\ntype: spec\ndomain: BASE\nkeys: [ID]\ninput: {SRC: {path: src.csv, types: {ID: int}}}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
    let identity = || yamaa_adapters::specification_report::Identity {
        runtime: "rust",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: "domain",
        specification: "child.yaml",
        base_directory: ".",
    };
    let scratch = crate::support::Scratch::canonical("malformed-parent");
    scratch.write("std/base.yaml", PARENT);
    let directory = scratch.to_path_buf();
    let child = directory.join("child.yaml");
    let child = child.to_str().unwrap();
    let header = "yamaa: \"1.0\"\ntype: spec\n";
    // The entry writes the `parent`: by its path through the local file
    // resolver, and through the caller's by the name it serves it under.
    let local = child.replace('\\', "/");
    for parent in [
        "std//base.yaml",
        "std/base.yaml/",
        "//server/share/std/base.yaml",
        "std\\base.yaml",
    ] {
        let entry = format!("{header}parent: '{parent}'\ndomain: CHILD\n");
        let refused = |issues: &[yamaa_core::diagnostic::Diagnostic], route: &str, source: &str| {
            assert_eq!(issues.len(), 1, "{parent} {route}: {issues:?}");
            let issue = &issues[0];
            assert_eq!(
                (issue.phase(), issue.condition()),
                ("validation", "resource_path_not_normalized"),
                "{parent} {route}"
            );
            assert_eq!(issue.spec_paths, ["parent"], "{parent} {route}");
            assert_eq!(
                yamaa_core::json::Value::Object(issue.context.clone()),
                yamaa_adapters::json!({"path": parent, "source": source}),
                "{parent} {route}"
            );
        };
        // Through the local file resolver.
        std::fs::write(child, &entry).unwrap();
        let Err(issues) =
            yamaa_adapters::resolved_artifact::resolve(child, None, Some(Language::Python), None)
        else {
            panic!("{parent}: resolved a malformed parent");
        };
        refused(&issues, "resolve", &local);
        let resources = Resources::for_entry(directory.to_str().unwrap()).unwrap();
        let Err(Prepared::PreparationFinding { diagnostic, .. }) =
            FileSpecification::prepare(resources, "child.yaml")
        else {
            panic!("{parent}: the refusal lost its finding");
        };
        refused(&[*diagnostic], "prepare", &local);
        // Through a caller's resolver that serves the parent under that name.
        let served = || {
            let files = MemoryResolver::default();
            files.insert(
                "/",
                "child.yaml",
                "memory:/child.yaml",
                Arc::from(entry.as_bytes()),
            );
            files.insert(
                "/child.yaml",
                parent,
                "memory:/base.yaml",
                Arc::from(PARENT),
            );
            Arc::new(Asked {
                files,
                written: std::sync::Mutex::new(Vec::new()),
            })
        };
        let resolver = served();
        let Err(error) = yamaa_adapters::file_workflow::select_with_resolver(
            request_named("child.yaml"),
            Some(Language::Python),
            resolver.clone(),
        ) else {
            panic!("{parent}: a caller's resolver selected a malformed parent");
        };
        refused(
            &yamaa_adapters::project_application::selection_failure(&error),
            "select through a resolver",
            "/child.yaml",
        );
        assert!(
            resolver
                .written
                .lock()
                .unwrap()
                .iter()
                .all(|written| written == "child.yaml"),
            "{parent}: {:?}",
            resolver.written.lock().unwrap()
        );
        refused(
            &yamaa_adapters::file_application::check_with_resolver(
                Some(Language::Python),
                request_named("child.yaml"),
                identity(),
                Some(served()),
            ),
            "check through a resolver",
            "/child.yaml",
        );
        let Err(issues) = yamaa_adapters::resolved_artifact::resolve(
            "child.yaml",
            None,
            Some(Language::Python),
            Some(served()),
        ) else {
            panic!("{parent}: resolved a malformed parent through a resolver");
        };
        refused(&issues, "resolve through a resolver", "/child.yaml");
        let Err(Prepared::PreparationFinding { diagnostic, .. }) = FileSpecification::prepare(
            Resources::with_resolver(served(), "child.yaml"),
            "child.yaml",
        ) else {
            panic!("{parent}: the refusal through a resolver lost its finding");
        };
        refused(&[*diagnostic], "prepare through a resolver", "/child.yaml");
    }
}

/// Seam: symbolic links made on a Unix host and the link kinds a caller's
/// resolver reports, which no case can stage. FILE-8 and LAYER-2: a `parent`
/// that reaches a symbolic link, as the layer's own name or as a directory on
/// its way, fails validation with `resource_path_symlink` at `parent`, naming
/// the path as written and the layer that writes it, as every other declared
/// file a run reads does, never with the engine's own refusal (#3030 D73). A
/// caller's resolver that reports a link, a junction or another reparse point
/// for the parent is refused from that report alone (FILE-7).
#[cfg(feature = "yaml")]
#[test]
fn a_linked_parent_is_refused_at_parent_on_every_route() {
    use yamaa_adapters::file_preparation::{Error as Prepared, FileSpecification};
    use yamaa_adapters::host_resources::ResourceKind;
    use yamaa_core::project_function::Language;
    const HEADERS: [(&str, &str); 1] = [("1.0", "yamaa: \"1.0\"\ntype: spec\n")];
    let entry = |header: &str, parent: &str| format!("{header}parent: '{parent}'\ndomain: CHILD\n");
    let identity = || yamaa_adapters::specification_report::Identity {
        runtime: "rust",
        runtime_version: "test",
        engine_version: yamaa_core::VERSION,
        example: "domain",
        specification: "child.yaml",
        base_directory: ".",
    };
    // The entry writes the `parent`, and the finding names it as `source`: by
    // its path through the local file resolver, and through a caller's by the
    // name the resolver is asked for the parent from, `/child.yaml`.
    let refused =
        |issues: &[yamaa_core::diagnostic::Diagnostic], parent: &str, source: &str, route: &str| {
            assert_eq!(issues.len(), 1, "{parent} {route}: {issues:?}");
            let issue = &issues[0];
            assert_eq!(
                (issue.phase(), issue.condition(), issue.rule()),
                ("validation", "resource_path_symlink", Some("FILE-8")),
                "{parent} {route}"
            );
            assert_eq!(issue.spec_paths, ["parent"], "{parent} {route}");
            assert_eq!(
                yamaa_core::json::Value::Object(issue.context.clone()),
                yamaa_adapters::json!({"path": parent, "source": source}),
                "{parent} {route}"
            );
        };
    // Through the local file resolver: a link in the layer's own name, in a
    // directory on its way, and a link whose target does not exist.
    #[cfg(unix)]
    {
        const PARENT: &[u8] = b"yamaa: '1.0'\ntype: spec\ndomain: BASE\nkeys: [ID]\ninput: {SRC: {path: src.csv, types: {ID: int}}}\noutput: {path: out.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";
        // Removed when the test ends, by a panic too.
        let scratch = crate::support::Scratch::new("linked-parent");
        std::fs::create_dir_all(scratch.join("std")).unwrap();
        std::fs::write(scratch.join("std/base.yaml"), PARENT).unwrap();
        std::os::unix::fs::symlink("std/base.yaml", scratch.join("base.yaml")).unwrap();
        std::os::unix::fs::symlink("std", scratch.join("linked")).unwrap();
        std::os::unix::fs::symlink("std/absent.yaml", scratch.join("dangling.yaml")).unwrap();
        let directory = yamaa_adapters::canonical_path::canonicalize(&scratch).unwrap();
        let child = directory.join("child.yaml");
        let child = child.to_str().unwrap();
        let local = child.to_owned();
        for (surface, header) in HEADERS {
            for parent in ["base.yaml", "linked/base.yaml", "dangling.yaml"] {
                let route = |name: &str| format!("{surface} {name}");
                std::fs::write(child, entry(header, parent)).unwrap();
                let request = || yamaa_core::build::Request {
                    specification: child,
                    environment: None,
                };
                let Err(error) = yamaa_adapters::file_workflow::select_request(
                    request(),
                    Some(Language::Python),
                ) else {
                    panic!("{surface} {parent}: a linked parent was selected");
                };
                refused(
                    &yamaa_adapters::project_application::selection_failure(&error),
                    parent,
                    &local,
                    &route("select"),
                );
                refused(
                    &yamaa_adapters::file_application::check(
                        Language::Python,
                        request(),
                        identity(),
                    ),
                    parent,
                    &local,
                    &route("check"),
                );
                let Err(issues) = yamaa_adapters::resolved_artifact::resolve(
                    child,
                    None,
                    Some(Language::Python),
                    None,
                ) else {
                    panic!("{surface} {parent}: resolved a linked parent");
                };
                refused(&issues, parent, &local, &route("resolve"));
                let resources = Resources::for_entry(directory.to_str().unwrap()).unwrap();
                let Err(Prepared::PreparationFinding { diagnostic, .. }) =
                    FileSpecification::prepare(resources, "child.yaml")
                else {
                    panic!("{surface} {parent}: the refusal lost its finding");
                };
                refused(&[*diagnostic], parent, &local, &route("prepare"));
            }
        }
    }
    // Through a caller's resolver that reports the kind of entry the parent
    // reaches.
    for (surface, header) in HEADERS {
        for kind in [
            ResourceKind::SymbolicLink,
            ResourceKind::Junction,
            ResourceKind::ReparsePoint,
        ] {
            let route = |name: &str| format!("{surface} {name} ({kind:?})");
            let served = || {
                let files = MemoryResolver::default();
                files.insert(
                    "/",
                    "child.yaml",
                    "memory:/child.yaml",
                    Arc::from(entry(header, "base.yaml").as_bytes()),
                );
                files.insert_kind("/child.yaml", "base.yaml", kind);
                Arc::new(files)
            };
            let Err(error) = yamaa_adapters::file_workflow::select_with_resolver(
                request_named("child.yaml"),
                Some(Language::Python),
                served(),
            ) else {
                panic!("{surface} {kind:?}: a caller's resolver selected a linked parent");
            };
            refused(
                &yamaa_adapters::project_application::selection_failure(&error),
                "base.yaml",
                "/child.yaml",
                &route("select"),
            );
            refused(
                &yamaa_adapters::file_application::check_with_resolver(
                    Some(Language::Python),
                    request_named("child.yaml"),
                    identity(),
                    Some(served()),
                ),
                "base.yaml",
                "/child.yaml",
                &route("check"),
            );
            let Err(issues) = yamaa_adapters::resolved_artifact::resolve(
                "child.yaml",
                None,
                Some(Language::Python),
                Some(served()),
            ) else {
                panic!("{surface} {kind:?}: resolved a linked parent through a resolver");
            };
            refused(&issues, "base.yaml", "/child.yaml", &route("resolve"));
            let Err(Prepared::PreparationFinding { diagnostic, .. }) = FileSpecification::prepare(
                Resources::with_resolver(served(), "child.yaml"),
                "child.yaml",
            ) else {
                panic!("{surface} {kind:?}: the refusal through a resolver lost its finding");
            };
            refused(
                &[*diagnostic],
                "base.yaml",
                "/child.yaml",
                &route("prepare"),
            );
        }
    }
}

#[cfg(feature = "yaml")]
fn request_named(specification: &str) -> yamaa_core::build::Request<'_> {
    yamaa_core::build::Request {
        specification,
        environment: None,
    }
}
