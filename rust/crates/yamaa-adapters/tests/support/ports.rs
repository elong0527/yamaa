//! Scripted doubles for the ports the adapters' suites drive, one per port:
//! a test scripts what each answers and reads back what the engine asked.
//! A test that needs an answer these cannot script keeps its own double.
use std::{collections::BTreeMap, convert::Infallible, sync::Arc};
use yamaa_core::{
    function_invocation::{Argument, HostError},
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    project_activation::ActivationPort,
    project_environment::LockReference,
    project_function::Language,
    resource::ResourceFailure,
    specification::SourceDeclaration,
    specification_run::{SourceOrigin, SpecificationSourcePort},
    value::Value,
};

/// An activation port a run must never reach: a specification that selects
/// no project function verifies no lock, binds nothing and calls nothing.
pub struct NoActivation;

impl ActivationPort for NoActivation {
    type Handle = ();
    type Error = Infallible;
    fn verify_lock(
        &mut self,
        _: Language,
        _: &LockReference,
        _: &[ProjectFunctionIdentity],
    ) -> Result<(), Infallible> {
        panic!("a run without project functions verified a lock")
    }
    fn bind(
        &mut self,
        _: &ProjectFunctionIdentity,
        _: &LogicalSignature,
    ) -> Result<(), Infallible> {
        panic!("a run without project functions bound a function")
    }
    fn invoke(&mut self, _: &(), _: &[Argument<'_>]) -> Result<Value, HostError<Infallible>> {
        panic!("a run without project functions called a function")
    }
}

/// A source port that serves authored bytes and records every capture.
/// `by_name` serves each input by its declared name, `by_path` by its
/// written path, and `any` serves the same bytes to every input. A capture
/// of an input the double does not serve fails the test, so a source a test
/// did not script can never be read.
pub struct Sources {
    bytes: BTreeMap<String, Arc<[u8]>>,
    key: Key,
    captured: Vec<String>,
}

enum Key {
    Name,
    Path,
    Any,
}

impl Sources {
    /// Bytes per input name.
    pub fn by_name<'a>(sources: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Self {
        Self::new(sources, Key::Name)
    }

    /// Bytes per written path.
    pub fn by_path<'a>(sources: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Self {
        Self::new(sources, Key::Path)
    }

    /// The same bytes for every input.
    pub fn any(bytes: &[u8]) -> Self {
        Self::new([("", bytes)], Key::Any)
    }

    fn new<'a>(sources: impl IntoIterator<Item = (&'a str, &'a [u8])>, key: Key) -> Self {
        Self {
            bytes: sources
                .into_iter()
                .map(|(key, bytes)| (key.to_owned(), Arc::from(bytes)))
                .collect(),
            key,
            captured: Vec::new(),
        }
    }

    /// The names of the inputs captured, in order, one entry per capture.
    pub fn captured(&self) -> &[String] {
        &self.captured
    }

    /// How many captures the engine made.
    pub fn reads(&self) -> usize {
        self.captured.len()
    }
}

impl SpecificationSourcePort for Sources {
    type Error = Infallible;
    fn resource_failure(&self, _: &Infallible) -> Option<ResourceFailure> {
        None
    }
    fn inspect(&mut self, _: &SourceDeclaration) -> Result<(), Infallible> {
        Ok(())
    }
    fn capture_reads(&self) -> usize {
        self.reads()
    }
    fn capture(
        &mut self,
        source: &SourceDeclaration,
        origin: &mut Option<SourceOrigin>,
    ) -> Result<Arc<[u8]>, Infallible> {
        *origin = Some(SourceOrigin::Resource);
        self.captured.push(source.name.clone());
        let key = match self.key {
            Key::Name => source.name.as_str(),
            Key::Path => source.path.as_str(),
            Key::Any => "",
        };
        let bytes = self.bytes.get(key).unwrap_or_else(|| {
            panic!(
                "the test serves no bytes for input {} at {}",
                source.name, source.path
            )
        });
        Ok(Arc::clone(bytes))
    }
}
