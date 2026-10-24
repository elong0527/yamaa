//! The separately loaded engine of the test-only loaded mode (#3702; the
//! design on #2870, sections 3.1 and 7): the loader takes the engine library
//! from beside a module's file and from nowhere else, each fixture engine of
//! `examples/`, loaded as a binding in the loaded mode loads one, negotiates
//! the generated request to its authored outcome in the shared truth's
//! `interface` family, an accepted one answering through its own table.
use std::env::consts::{DLL_PREFIX, DLL_SUFFIX};
use std::path::{Path, PathBuf};

use yamaa_boundary::client::{Census, Gate};
use yamaa_boundary::loader::{self, Missing, LIBRARY};
use yamaa_boundary::{HandleKind, Value};

use crate::support::{family, host_request, outcome_record, resolved, serial, Scratch};

/// A fixture engine of `examples/`, which `cargo test -p yamaa-engine`
/// builds beside its test binary.
fn fixture(name: &str) -> PathBuf {
    let binary = std::env::current_exe().unwrap();
    let profile = binary.parent().and_then(Path::parent).unwrap();
    let library = profile
        .join("examples")
        .join(format!("{DLL_PREFIX}{name}{DLL_SUFFIX}"));
    assert!(
        library.is_file(),
        "{} is not built: `cargo test -p yamaa-engine` builds the crate's examples",
        library.display()
    );
    library
}

/// Place `library` in `directory` under the engine library's name: linked
/// where the platform allows, else copied.
fn place(library: &Path, directory: &Path) {
    let placed = directory.join(LIBRARY);
    #[cfg(unix)]
    std::os::unix::fs::symlink(library, &placed).unwrap();
    #[cfg(not(unix))]
    if std::fs::hard_link(library, &placed).is_err() {
        std::fs::copy(library, &placed).unwrap();
    }
}

/// The generated request, as the hosts send it.
fn generated() -> Value {
    host_request(&resolved(&family("interface")["request"]))
}

/// The handles of `kind` an engine's census counts.
fn counted(census: Census, kind: HandleKind) -> Option<u64> {
    census
        .handles
        .into_iter()
        .find(|(counted, _)| *counted == kind)
        .map(|(_, count)| count)
}

/// The shape of a load: the variant of its answer and the path it names,
/// without the system's own words.
fn shape(
    load: Result<yamaa_boundary::client::Negotiate, Missing>,
) -> (&'static str, Option<PathBuf>) {
    match load {
        Ok(_) => ("loaded", None),
        Err(Missing::Module(_)) => ("no absolute module", None),
        Err(Missing::Library(path, _)) => ("no library", Some(path)),
        Err(Missing::Entry(path)) => ("no entry", Some(path)),
    }
}

/// The current directory for as long as the value lives.
struct CurrentDirectory(PathBuf);

impl CurrentDirectory {
    fn enter(directory: &Path) -> Self {
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(directory).unwrap();
        Self(previous)
    }
}

impl Drop for CurrentDirectory {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

#[test]
fn each_fixture_engine_gets_its_authored_outcome() {
    // An accepted engine opens the gate on its own table, apart from the
    // engine this test binary links: a plans holder counts in the loaded
    // census alone, once, until the client gives it back. Each fixture loads
    // once, as a binding loads its engine once.
    for fixture_case in family("interface")["fixtures"].as_array().unwrap() {
        let label = fixture_case["fixture"].as_str().unwrap();
        let module = Scratch::new("loading");
        place(&fixture(fixture_case["library"].as_str().unwrap()), &module);
        let gate = Gate::new();
        // SAFETY: the library beside the module is a fixture engine of this
        // crate.
        let outcome = unsafe { gate.negotiate_beside(&generated(), &module.join("module")) };
        let table = gate.pass().ok().map(|engine| {
            let before = counted(engine.census(), HandleKind::Plans);
            let plans = engine.plans().unwrap();
            let held = counted(engine.census(), HandleKind::Plans);
            drop(plans);
            (before, held, counted(engine.census(), HandleKind::Plans))
        });
        let expected = resolved(&fixture_case["outcome"]);
        let accepted = expected["accepted"] == true;
        assert_eq!(
            (outcome_record(&outcome), table),
            (expected, accepted.then_some((Some(0), Some(1), Some(0)))),
            "{label}"
        );
    }
}

#[test]
fn the_loader_never_searches_the_current_directory() {
    // An engine the module's directory lacks is missing, though the current
    // directory holds one under the engine's name; a module path that is
    // not absolute, which would name a file there, is refused unread.
    let _serial = serial();
    let (module, current) = (Scratch::new("module"), Scratch::new("current"));
    place(&fixture("fixture_no_format_parquet"), &current);
    let _entered = CurrentDirectory::enter(&current);
    let absolute = module.join("module");
    // SAFETY: no library is beside the module, and the one in the current
    // directory is a fixture engine of this crate.
    let (refused, beside, relative) = unsafe {
        (
            Gate::new().negotiate_beside(&generated(), &absolute),
            loader::load(&absolute),
            loader::load(Path::new("module")),
        )
    };
    assert_eq!(
        (shape(beside), shape(relative), outcome_record(&refused)),
        (
            ("no library", Some(module.join(LIBRARY))),
            ("no absolute module", None),
            resolved(&serde_json::json!({
                "accepted": false,
                "requested_version": "{version}",
                "supported_version": null,
                "reason": "missing_interface",
                "missing_features": [],
            })),
        ),
        "an engine in the current directory alone"
    );
}
