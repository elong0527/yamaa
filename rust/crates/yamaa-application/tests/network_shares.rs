//! Windows network shares behind the file rules (D15): a specification, its
//! inputs, its project file and its outputs on a UNC path are read and saved
//! through the local file resolver, and links, junctions and other reparse
//! points on a share stay refused (FILE-8). Each study reaches its temporary
//! directory through the administrative share of its drive
//! (`\\localhost\C$\...`). A host where that share is unreachable fails these
//! tests, so CI's Windows lanes never pass them unrun; a developer without the
//! share sets `YAMAA_SKIP_NETWORK_SHARES=1` to skip them with a message, and no
//! CI lane sets it. The path classification these walks rely on is tested on
//! every host in `file_resolver.rs`.
#![cfg(all(windows, feature = "yaml"))]

use std::{
    fs,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
};
use yamaa_adapters::{file_configuration, file_resources::Error};
use yamaa_application::{file_application, specification_report::Identity};
use yamaa_core::{domain_entry::Request, project_function::Language};

const SPEC: &str = "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {SRC: {path: ../data/src.csv, types: {ID: int}}}\noutput: {path: out/result.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: int, derivation: SRC.ID}\n";

/// A temporary study and its spelling through the drive's administrative
/// share.
struct Study {
    local: crate::support::Scratch,
    share: PathBuf,
}
impl Study {
    fn new() -> Option<Self> {
        let local = crate::support::Scratch::canonical("network-share");
        for directory in ["study/out", "data"] {
            fs::create_dir_all(local.join(directory)).unwrap();
        }
        fs::write(local.join("study/spec.yaml"), SPEC).unwrap();
        fs::write(local.join("data/src.csv"), "ID\n1\n2\n").unwrap();
        let text = local.to_str().unwrap();
        let (drive, rest) = text.split_once(":\\").unwrap();
        let share = PathBuf::from(format!(r"\\localhost\{drive}$\{rest}"));
        if !fs::metadata(&share).is_ok_and(|metadata| metadata.is_dir()) {
            let _ = fs::remove_dir_all(&local);
            if std::env::var_os("YAMAA_SKIP_NETWORK_SHARES").is_some_and(|skip| skip == "1") {
                eprintln!("skipped: the administrative share {share:?} is not reachable");
                return None;
            }
            panic!(
                "the administrative share {share:?} is not reachable; set \
                 YAMAA_SKIP_NETWORK_SHARES=1 to skip the network share tests"
            );
        }
        Some(Self { local, share })
    }
    fn share(&self, name: &str) -> String {
        self.share.join(name).to_str().unwrap().to_owned()
    }
}

fn identity(specification: &str) -> Identity<'_> {
    Identity {
        runtime: "test",
        runtime_version: "test",
        engine_version: "test",
        example: "test",
        specification,
        base_directory: ".",
    }
}

#[test]
fn a_specification_on_a_network_share_builds_and_saves_through_the_share() {
    let Some(study) = Study::new() else {
        return;
    };
    let specification = study.share(r"study\spec.yaml");
    // The entry's identity names the share: its parent is the selected
    // directory, and `../data/src.csv` leaves it to walk from the share root.
    let (resources, entry) = file_configuration::resources(&specification).unwrap();
    let identity_text = resources.resolve(&entry).unwrap();
    assert!(
        identity_text
            .to_ascii_lowercase()
            .starts_with("//localhost/"),
        "{identity_text}"
    );
    let mut result = crate::support::study::domain(
        Language::Python,
        Request {
            specification: &specification,
            environment: None,
        },
        identity(&specification),
    )
    .unwrap();
    assert!(result.issues().is_empty(), "{:?}", result.issues());
    assert!(result.output().is_some());
    assert!(result.save().unwrap());
    assert_eq!(
        fs::read(study.local.join(r"study\out\result.csv")).unwrap(),
        b"ID\n1\n2\n"
    );
}

#[test]
fn a_project_file_on_a_network_share_is_a_caller_path() {
    let Some(study) = Study::new() else {
        return;
    };
    fs::write(
        study.local.join(r"study\project.yaml"),
        "yamaa: \"1.0\"\ntype: project\nsdtm:\n  specs: [spec.yaml]\n",
    )
    .unwrap();
    let specification = study.share(r"study\spec.yaml");
    let project = study.share(r"study\project.yaml");
    let checked = file_application::check(
        Language::Python,
        Request {
            specification: &specification,
            environment: Some(&project),
        },
        identity(&specification),
    );
    assert!(checked.is_empty(), "{checked:?}");
    let read =
        yamaa_application::project_scope::Project::read(&project, Some(Language::Python), None)
            .unwrap();
    assert_eq!(read.listed().len(), 1);
    assert!(read.listed()[0].failure().is_empty());
}

#[test]
fn links_on_a_network_share_stay_refused() {
    let Some(study) = Study::new() else {
        return;
    };
    let link = study.local.join(r"study\linked");
    let target = study.local.join("data");
    let output = std::process::Command::new("cmd")
        .arg("/d")
        .arg("/c")
        .raw_arg(format!(
            "mklink /J \"{}\" \"{}\"",
            link.display(),
            target.display()
        ))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let (mut resources, _) =
        file_configuration::resources(&study.share(r"study\spec.yaml")).unwrap();
    assert_eq!(resources.capture("linked/src.csv"), Err(Error::Symlink));
    assert!(resources.capture("../data/src.csv").unwrap().1);
    fs::remove_dir(link).unwrap();
    assert!(Path::new(&study.share(r"data\src.csv")).is_file());
}
