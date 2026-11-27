//! A study directory through the file route every host takes: the static
//! check, then preparation and a run, through an in-memory resolver. Each
//! input is a directory: the entry specification `spec.yaml`, then each
//! other file as a NUL byte, its path from the directory, a line feed and its
//! bytes (YAML text holds no NUL, nor does the small text data a run reads).
//! A file named `project.yaml` is the project of both calls, so its layers,
//! function files, codelist files, analysis results files and lock are read
//! and admitted as their own file types, and the specification compiles
//! against them; the run then reads its inputs and evaluates every
//! derivation. The check is a Python host's and the run an R host's, so each
//! reads its own `environments` entry; the run's host-functions port is the
//! empty one, which refuses a call to a project function before any input is
//! read, as the command line's does.
#![no_main]

use std::{collections::BTreeMap, convert::Infallible, sync::Arc};

use libfuzzer_sys::fuzz_target;
use yamaa_adapters::host_resources::MemoryResolver;
use yamaa_application::{file_application, specification_report::Identity};
use yamaa_core::{
    application_observer::Scope, domain_entry::Request, host_functions::Empty,
    project_function::Language,
};

const ENTRY: &str = "spec.yaml";
const PROJECT: &str = "project.yaml";

/// The files of an input by their paths from the directory, the entry first.
fn directory(data: &[u8]) -> BTreeMap<String, &[u8]> {
    let mut parts = data.split(|&byte| byte == 0);
    let mut files = BTreeMap::from([(ENTRY.to_owned(), parts.next().unwrap_or_default())]);
    for part in parts {
        let Some(at) = part.iter().position(|&byte| byte == b'\n') else {
            continue;
        };
        if let Ok(path) = std::str::from_utf8(&part[..at]) {
            files.entry(path.to_owned()).or_insert(&part[at + 1..]);
        }
    }
    files
}

/// A resolver that serves each file to the directory itself and to every
/// file in it, by the path that file would write: from its own directory,
/// up to the root, then down to the file served.
fn resolver(files: &BTreeMap<String, &[u8]>) -> MemoryResolver {
    let resolver = MemoryResolver::default();
    let declaring = std::iter::once(None).chain(files.keys().map(Some));
    for from in declaring {
        let (identity, up) = match from {
            None => ("/".to_owned(), String::new()),
            Some(path) => (format!("/{path}"), "../".repeat(path.matches('/').count())),
        };
        for (path, bytes) in files {
            resolver.insert(
                &identity,
                &format!("{up}{path}"),
                &format!("memory:/{path}"),
                Arc::from(*bytes),
            );
        }
    }
    resolver
}

fuzz_target!(|data: &[u8]| {
    let files = directory(data);
    let resolver = Arc::new(resolver(&files));
    let request = || Request {
        specification: ENTRY,
        environment: files.contains_key(PROJECT).then_some(PROJECT),
    };
    let _ =
        file_application::check_located(Some(Language::Python), request(), Some(resolver.clone()));
    let Ok(prepared) = file_application::select(Some(Language::R), request(), Some(resolver))
    else {
        return;
    };
    let mut run = prepared.run_scope(None);
    let identity = Identity {
        runtime: "fuzz",
        runtime_version: "0",
        engine_version: yamaa_core::VERSION,
        example: "study",
        specification: ENTRY,
        base_directory: ".",
    };
    let _ = file_application::run(
        &prepared,
        &mut run,
        &mut Empty::<Infallible>::new(),
        |_| file_application::Classification::Opaque,
        identity,
        Scope::default(),
    );
});
