//! YAML specifications: the vendored YAML parser, schema admission,
//! normalization, inheritance and compilation with its static checks. Each
//! input is an entry specification that a host would check, prepared through
//! the same shipped-schema entry as the host route. The engine admits the
//! shipped 1.0 specification closure once per process, so every execution
//! spends its time on the input.
#![no_main]

use libfuzzer_sys::fuzz_target;
use yamaa_adapters::{shipped_schema, specification_source::Source};
use yamaa_application::specification_run::PreparedRun;
use yamaa_core::language::FileType;
use yamaa_core::resolver::{Captured, Failure, Located, Name, Resolver};
use yamaa_core::resource::ResourceFailure;

/// A resolver that holds only the entry: every parent and dictionary it names
/// is missing, as for a host resolver that lacks the file.
struct EntryOnly;

impl Resolver for EntryOnly {
    type Error = ();
    fn inspect(&mut self, _name: Name<'_>) -> Result<Located, ()> {
        Err(())
    }
    fn capture(&mut self, _name: Name<'_>) -> Result<Captured, ()> {
        Err(())
    }
    fn failure(&self, _name: Name<'_>, _error: &()) -> Option<Failure> {
        Some(Failure::Resource(ResourceFailure::Missing))
    }
    fn rebase(&mut self, _layer: &Located, _entry: &Located, written: &str) -> Result<String, ()> {
        Ok(written.into())
    }
}

fuzz_target!(
    init: {
        // Admit the closure before the first input, and stop the run if it
        // cannot be: every input would then be refused before it is read.
        shipped_schema::capture_versioned(FileType::Spec)
            .expect("the shipped specification closure admits");
    },
    |data: &[u8]| {
        let source = Source {
            identity: "spec.yaml".into(),
            bytes: data.to_vec(),
        };
        let Ok(document) = shipped_schema::prepare(source, "spec.yaml".into(), &mut EntryOnly)
        else {
            return;
        };
        if let Ok(run) = PreparedRun::prepare(document) {
            let _ = run.check_diagnostics();
        }
    }
);
