//! The lint over files and directories: which files a path expands to, the
//! order they keep, and what a fix writes or refuses to write.

use super::{super::lint, CANONICAL, VERSIONED};
use std::path::{Path, PathBuf};
use yamaa_core::diagnostic::ConditionCode;

struct Directory(PathBuf);

impl Directory {
    fn new(name: &str) -> Self {
        Self::under(&std::env::temp_dir(), name)
    }

    fn under(root: &Path, name: &str) -> Self {
        let path = root.join(format!(
            "yamaa-lint-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn display(path: &Path) -> String {
    std::path::absolute(path)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn directories_expand_to_specifications_in_sorted_order() {
    let directory = Directory::new("expand");
    let broken = CANONICAL.replace("base: DM\n\noutput:", "base: DM\noutput:");
    let second = directory.write("b/adae.yaml", &broken);
    let first = directory.write("a-b.yaml", &broken);
    let third = directory.write("b/c/adsl.yaml", &broken);
    directory.write(
        "environment.yaml",
        "schema_version: \"1.0\"\nstudy: {id: S1}\n",
    );
    directory.write("yamaa-project.yaml", "data_roots: [data]\n");
    directory.write(".cache/copy.yaml", &broken);
    directory.write("sequence-key.yaml", "? [a, b]\n: 1\nschema_version: 1\n");
    directory.write("mapping-key.yaml", "? {a: 1}\n: 2\n");
    directory.write("notes.txt", &broken);
    let linted = lint(&[text(&directory.0)], false).unwrap();
    assert_eq!(
        linted
            .iter()
            .map(|file| file.file.clone())
            .collect::<Vec<_>>(),
        [display(&first), display(&second), display(&third)]
    );
    assert!(linted
        .iter()
        .all(|file| file.findings.len() == 1 && file.findings[0].finding == "root_spacing"));
}

#[test]
fn a_fragment_linted_with_a_file_that_includes_it_is_a_layer() {
    // A fragment's value may replace one of the layers it composes on, so
    // a call that also lints a specification whose composition includes
    // it reads the fragment as a layer: its restated default is not
    // reported and no fix removes it (LAYER-3). Linted alone, the same
    // file is read as standalone.
    let directory = Directory::new("fragments");
    let fragment_text = "yamaa: \"1.0\"\ntype: spec\ninput:\n  AE: {path: ae.csv, \
                         types: {AESEQ: int}, empty_string: missing}\n";
    let fragment = directory.write("shared/fragment.yaml", fragment_text);
    directory.write(
        "adae.yaml",
        "yamaa: \"1.0\"\ntype: spec\ninclude: [shared/fragment.yaml]\n",
    );
    let alone = lint(&[text(&fragment)], false).unwrap();
    assert_eq!(
        alone[0]
            .findings
            .iter()
            .map(|item| (item.finding, item.spec_path.as_str()))
            .collect::<Vec<_>>(),
        [("restated_default", "input.AE.empty_string")]
    );
    let together = lint(&[text(&directory.0)], true).unwrap();
    assert_eq!(
        together
            .iter()
            .map(|file| (file.file.clone(), file.findings.len(), file.fixed))
            .collect::<Vec<_>>(),
        [
            (display(&directory.0.join("adae.yaml")), 0, false),
            (display(&fragment), 0, false),
        ]
    );
    assert_eq!(std::fs::read_to_string(&fragment).unwrap(), fragment_text);
    // A fragment that the parent of a linted specification includes is a
    // layer too, though the call names neither the parent nor any other
    // file that includes it: the parent chain is followed.
    directory.write(
        "templates/template.yaml",
        "yamaa: \"1.0\"\ntype: spec\ninclude: [../shared/fragment.yaml]\n",
    );
    let entry = directory.write(
        "adcm.yaml",
        "yamaa: \"1.0\"\ntype: spec\nparent: templates/template.yaml\n",
    );
    let through = lint(&[text(&entry), text(&fragment)], true).unwrap();
    assert_eq!(
        through
            .iter()
            .map(|file| (file.file.clone(), file.findings.len(), file.fixed))
            .collect::<Vec<_>>(),
        [(display(&entry), 0, false), (display(&fragment), 0, false)]
    );
    assert_eq!(std::fs::read_to_string(&fragment).unwrap(), fragment_text);
}

#[test]
fn directories_expand_to_every_1_0_file_type_by_its_header() {
    let directory = Directory::new("expand-versioned");
    let broken = VERSIONED.replace("base: DM\n\noutput:", "base: DM\noutput:");
    let specification = directory.write(
        "adae.yaml",
        &CANONICAL.replace("base: DM\n\noutput:", "base: DM\noutput:"),
    );
    let entry = directory.write("adsl.yaml", &broken);
    // A layer declares the type of the chain; its unknown field is the
    // engine's to refuse, not discovery's.
    let layer = directory.write("layers/base.yaml", &format!("{broken}\nunknown: 1\n"));
    let project = directory.write("project.yaml", "yamaa: \"1.0\"\ntype: project\nsdtm: {}\n");
    let function = directory.write(
        "functions/bmi.yaml",
        "yamaa: \"1.0\"\ntype: function\nname: bmi\ntests: []\n",
    );
    let codelists = directory.write(
        "codelists/sponsor.yaml",
        "yamaa: \"1.0\"\ntype: codelists\ncodelists: []\n",
    );
    // A type no file has, no type, and a mapping dictionary, which
    // declares no header, are no yamaa files.
    directory.write("study.yaml", "yamaa: \"1.0\"\ntype: study\nsdtm: {}\n");
    directory.write("untyped.yaml", &broken.replace("type: spec\n", ""));
    directory.write("dictionaries/sex.yaml", "F: Female\nM: Male\n");
    // SCHEMA-2, #3443: directory discovery admits only the 1.x header.
    directory.write(
        "legacy.yaml",
        &CANONICAL.replacen(
            "yamaa: \"1.0\"\ntype: spec\n",
            "schema_version: \"1.0\"\n",
            1,
        ),
    );
    // A root that declares `schema_version` beside `yamaa` is no 1.x
    // file, as the engine reads it.
    directory.write(
        "both.yaml",
        &broken.replace(
            "yamaa: \"1.0\"\n",
            "schema_version: \"1.0\"\nyamaa: \"1.0\"\n",
        ),
    );
    let linted = lint(&[text(&directory.0)], false).unwrap();
    assert_eq!(
        linted
            .iter()
            .map(|file| (
                file.file.clone(),
                file.findings
                    .iter()
                    .map(|item| (item.finding, item.spec_path.as_str()))
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>(),
        [
            (display(&specification), vec![("root_spacing", "output")]),
            (display(&entry), vec![("root_spacing", "output")]),
            (display(&codelists), vec![("root_spacing", "codelists")]),
            (display(&function), vec![("root_spacing", "tests")]),
            (display(&layer), vec![("root_spacing", "output")]),
            (display(&project), vec![("root_spacing", "sdtm")]),
        ]
    );
}

#[test]
fn a_named_file_is_always_linted_and_files_keep_argument_order() {
    let directory = Directory::new("named");
    let clean = directory.write("clean.yaml", CANONICAL);
    // A named file without the 1.x header is still read.
    let notes = directory.write("notes.yaml", "domain: TEST\nstudy: {id: S1}\n");
    let linted = lint(&[text(&notes), text(&clean)], false).unwrap();
    assert_eq!(linted.len(), 2);
    assert_eq!(linted[0].file, display(&notes));
    assert_eq!(linted[0].findings[0].finding, "root_spacing");
    assert!(linted[1].findings.is_empty());
}

/// A named file written for 0.5 declares `schema_version`, which no
/// reader reads: the lint refuses the whole call as
/// `unsupported_language_version`, naming 0.5 (SCHEMA-2), and writes
/// nothing, the files named before it included.
#[test]
fn a_named_file_that_declares_schema_version_refuses_the_call() {
    let directory = Directory::new("named-0-5");
    let packed = CANONICAL.replace("base: DM\n\noutput:", "base: DM\noutput:");
    let fixable = directory.write("fixable.yaml", &packed);
    let legacy = directory.write(
        "legacy.yaml",
        &CANONICAL.replacen(
            "yamaa: \"1.0\"\ntype: spec\n",
            "schema_version: \"1.0\"\n",
            1,
        ),
    );
    let error = lint(&[text(&fixable), text(&legacy)], true).unwrap_err();
    assert_eq!(error.file(), Some(display(&legacy).as_str()));
    let diagnostics = error.diagnostics();
    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| (
                diagnostic.code,
                diagnostic.spec_paths.clone(),
                yamaa_core::json::Value::Object(diagnostic.context.clone()),
                diagnostic.source_span.is_none() && diagnostic.operand_route.is_none(),
            ))
            .collect::<Vec<_>>(),
        [(
            yamaa_core::diagnostic::ConditionCode::HeaderLanguageVersion,
            vec!["schema_version".to_string()],
            crate::json!({"version": "0.5", "language_versions": ["1.0"]}),
            true,
        )]
    );
    assert_eq!(std::fs::read_to_string(&fixable).unwrap(), packed);
}

#[cfg(unix)]
#[test]
fn directory_links_are_skipped_and_specification_links_are_refused() {
    use std::os::unix::fs::symlink;

    let directory = Directory::new("links");
    let packed_text = super::packed();
    let packed = directory.write("packed.yaml", &packed_text);
    let target = directory.write(".target/adsl.yaml", CANONICAL);
    symlink(target.parent().unwrap(), directory.0.join("linked.yaml")).unwrap();
    let linted = lint(&[text(&directory.0)], false).unwrap();
    assert_eq!(linted.len(), 1);
    assert_eq!(linted[0].file, display(&packed));

    let link = directory.0.join("linked-file.yaml");
    symlink(&target, &link).unwrap();
    let error = lint(&[text(&directory.0)], true).unwrap_err();
    assert_eq!(error.file(), Some(display(&link).as_str()));
    assert!(matches!(
        error,
        super::super::Error::Read {
            error: crate::file_resources::Error::Symlink,
            ..
        }
    ));
    assert_eq!(std::fs::read_to_string(&packed).unwrap(), packed_text);
    assert_eq!(std::fs::read_to_string(&target).unwrap(), CANONICAL);
}

#[cfg(unix)]
#[test]
fn directories_skip_special_files_and_links_to_them() {
    use std::os::unix::{
        fs::symlink,
        net::{SocketAddr, UnixListener},
    };

    // A socket's path must fit `sockaddr_un.sun_path` with a closing NUL:
    // 104 bytes on macOS and 108 on Linux. macOS's temporary directory,
    // `/var/folders/<2>/<30>/T`, is 48 bytes and leaves too little room,
    // so this directory goes under `/tmp` there. The socket path is then
    // at most 83 bytes: `/tmp/yamaa-lint-special-files-` (30), a u32
    // process id (10), `-ThreadId(` (10), a u64 thread id (20) and
    // `)/socket.yaml` (13). A path that still does not fit, such as one
    // under a long TMPDIR elsewhere, skips the socket case.
    let root = if cfg!(target_os = "macos") {
        PathBuf::from("/tmp")
    } else {
        std::env::temp_dir()
    };
    let directory = Directory::under(&root, "special-files");
    let packed_text = super::packed();
    let packed = directory.write("packed.yaml", &packed_text);
    let socket = directory.0.join("socket.yaml");
    if let Err(error) = SocketAddr::from_pathname(&socket) {
        eprintln!(
            "{}: {error}; special-file case not exercised",
            socket.display()
        );
        return;
    }
    let _listener = UnixListener::bind(&socket).unwrap();
    symlink(&socket, directory.0.join("linked-socket.yaml")).unwrap();

    for fix in [false, true] {
        let linted = lint(&[text(&directory.0)], fix).unwrap();
        assert_eq!(linted.len(), 1);
        assert_eq!(linted[0].file, display(&packed));
        assert_eq!(linted[0].fixed, fix);
        assert_eq!(
            std::fs::read_to_string(&packed).unwrap(),
            if fix { CANONICAL } else { &packed_text }
        );
    }
}

#[cfg(unix)]
#[test]
fn unreadable_directories_refuse_before_any_file_is_fixed() {
    use std::os::unix::fs::PermissionsExt;

    for nested in [false, true] {
        let directory = Directory::new("unreadable-directory");
        let packed_text = super::packed();
        let packed = directory.write("packed.yaml", &packed_text);
        directory.write("specs/nested/adsl.yaml", CANONICAL);
        let root = directory.0.join("specs");
        let blocked = if nested {
            root.join("nested")
        } else {
            root.clone()
        };
        let permissions = std::fs::metadata(&blocked).unwrap().permissions();
        std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o0)).unwrap();
        if std::fs::read_dir(&blocked).is_ok() {
            std::fs::set_permissions(&blocked, permissions).unwrap();
            eprintln!("process bypasses DAC; unreadable-directory case not exercised");
            return;
        }
        let results = [false, true].map(|fix| lint(&[text(&packed), text(&root)], fix));
        std::fs::set_permissions(&blocked, permissions).unwrap();
        for result in results {
            let error = result.unwrap_err();
            assert_eq!(error.file(), Some(display(&blocked).as_str()));
            assert!(matches!(
                error,
                super::super::Error::Read {
                    error: crate::file_resources::Error::Unreadable,
                    ..
                }
            ));
            assert_eq!(
                error.diagnostics(),
                [yamaa_core::application_issue::rejected(
                    yamaa_core::application_issue::Step::Prepare,
                    yamaa_core::application_issue::Fault::ResourceUnreadable
                )]
            );
        }
        assert_eq!(std::fs::read_to_string(&packed).unwrap(), packed_text);
    }
}

#[cfg(unix)]
#[test]
fn unreadable_yaml_candidates_refuse_before_any_file_is_fixed() {
    use std::os::unix::fs::PermissionsExt;

    let directory = Directory::new("unreadable-candidate");
    let packed_text = super::packed();
    let packed = directory.write("packed.yaml", &packed_text);
    let blocked = directory.write("specs/adsl.yaml", CANONICAL);
    let permissions = std::fs::metadata(&blocked).unwrap().permissions();
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o0)).unwrap();
    if std::fs::File::open(&blocked).is_ok() {
        std::fs::set_permissions(&blocked, permissions).unwrap();
        eprintln!("process bypasses DAC; unreadable-candidate case not exercised");
        return;
    }
    let results =
        [false, true].map(|fix| lint(&[text(&packed), text(&directory.0.join("specs"))], fix));
    std::fs::set_permissions(&blocked, permissions).unwrap();
    for result in results {
        let error = result.unwrap_err();
        assert_eq!(error.file(), Some(display(&blocked).as_str()));
        assert_eq!(
            error.diagnostics(),
            [yamaa_core::application_issue::rejected(
                yamaa_core::application_issue::Step::Prepare,
                yamaa_core::application_issue::Fault::ResourceUnreadable
            )]
        );
    }
    assert_eq!(std::fs::read_to_string(&packed).unwrap(), packed_text);
}

#[test]
fn fix_rewrites_only_the_files_it_proves_and_keeps_their_mode() {
    let directory = Directory::new("fix");
    let packed = directory.write(
        "packed.yaml",
        &CANONICAL.replace(
            "    derivation: DM.STUDYID\n\n",
            "    derivation: DM.STUDYID\n",
        ),
    );
    let clean = directory.write("clean.yaml", CANONICAL);
    // No umask gives a new file an execute bit, so only a kept mode
    // passes.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&packed, std::fs::Permissions::from_mode(0o744)).unwrap();
    }
    let linted = lint(&[text(&packed), text(&clean)], true).unwrap();
    assert_eq!(std::fs::read_to_string(&packed).unwrap(), CANONICAL);
    assert!(linted[0].fixed && linted[0].findings.is_empty());
    assert!(!linted[1].fixed);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&packed).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o744);
    }
    // Nothing staged is left beside the files.
    let mut names = std::fs::read_dir(&directory.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["clean.yaml", "packed.yaml"]);
}

#[test]
fn a_file_that_cannot_be_read_or_admitted_refuses_before_anything_is_written() {
    let directory = Directory::new("refuse");
    let packed_text = CANONICAL.replace(
        "    derivation: DM.STUDYID\n\n",
        "    derivation: DM.STUDYID\n",
    );
    let packed = directory.write("packed.yaml", &packed_text);
    let broken = directory.write("broken.yaml", "a: [\n");
    let error = lint(&[text(&packed), text(&broken)], true).unwrap_err();
    assert_eq!(error.file(), Some(display(&broken).as_str()));
    assert!(matches!(error, super::super::Error::Source { .. }));
    assert_eq!(
        error.diagnostics()[0].code,
        ConditionCode::SourceInvalidYaml
    );
    assert_eq!(std::fs::read_to_string(&packed).unwrap(), packed_text);
    let missing = directory.0.join("missing.yaml");
    let error = lint(&[text(&missing)], false).unwrap_err();
    assert!(matches!(error, super::super::Error::Read { .. }));
    let diagnostics = error.diagnostics();
    assert_eq!(diagnostics[0].code, ConditionCode::ResourceMissing);
    assert_eq!(
        diagnostics[0].context.get("path"),
        Some(&yamaa_core::json::Value::String(display(&missing)))
    );
    let ascii = directory.write("accent.yaml", "a: caf\u{e9}\n");
    let error = lint(&[text(&ascii)], false).unwrap_err();
    assert_eq!(error.diagnostics()[0].code, ConditionCode::SourceNonAscii);
}
