//! YAML specifications: the vendored YAML parser, schema admission,
//! normalization, inheritance and compilation with its static checks. Each
//! input is an entry specification that a host would check.
//!
//! A host check first admits the shipped schema closure, the same fixed work
//! for every call and most of its time. This target admits that closure once,
//! through the same capture, then prepares and compiles each input as the host
//! route does, so every execution spends its time on the input.
#![no_main]

use std::{
    convert::Infallible,
    sync::{Arc, LazyLock},
};

use libfuzzer_sys::fuzz_target;
use yamaa_adapters::{
    shipped_schema,
    specification_run::PreparedRun,
    specification_source::{CapturedSchema, InheritancePort, Source},
};
use yamaa_core::inheritance::{Source as Location, SourceError};

static SCHEMA: LazyLock<Arc<CapturedSchema>> =
    LazyLock::new(|| shipped_schema::capture().expect("the shipped schema closure admits"));

/// A resolver that holds only the entry: every parent and dictionary it names
/// is unavailable, as for a host resolver that lacks the file.
struct EntryOnly;

impl InheritancePort for EntryOnly {
    type Error = Infallible;
    fn canonicalize(
        &mut self,
        _declaring: &str,
        _written: &str,
    ) -> Result<Location, SourceError<Infallible>> {
        Err(SourceError::Unavailable)
    }
    fn capture(&mut self, _source: &Location) -> Result<Vec<u8>, SourceError<Infallible>> {
        Err(SourceError::Unavailable)
    }
    fn rebase(
        &mut self,
        _layer: &Location,
        _entry: &Location,
        written: &str,
    ) -> Result<String, Infallible> {
        Ok(written.into())
    }
}

fuzz_target!(
    init: {
        LazyLock::force(&SCHEMA);
    },
    |data: &[u8]| {
        let source = Source {
            identity: "spec.yaml".into(),
            bytes: data.to_vec(),
        };
        let Ok(document) = SCHEMA.prepare_document(source, "spec.yaml".into(), &mut EntryOnly)
        else {
            return;
        };
        if let Ok(run) = PreparedRun::prepare(document) {
            let _ = run.check_diagnostics();
        }
    }
);
