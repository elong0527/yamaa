//! Scripted doubles for the ports the application's suites drive, one per port:
//! a test scripts what each answers and reads back what the engine asked.
//! A test that needs an answer these cannot script keeps its own double.
use std::{collections::BTreeMap, convert::Infallible, sync::Arc};
use yamaa_core::{
    function_invocation::{Argument, HostError},
    function_signature::{LogicalSignature, ProjectFunctionIdentity},
    host_functions::{HostFunctions, Lock},
    project_environment::LockKind,
    project_function::Language,
    resolver::{Captured, Failure, Located, Name, Resolver},
    value::Value,
};

/// The least uv lock the project decoder reads: version 1 and no package.
pub const UV_LOCK: &str = "version = 1\npackage = []\n";
/// The least renv lock the project decoder reads: an R version and no
/// package.
pub const RENV_LOCK: &str = r#"{"R":{"Version":"4.6.1"},"Packages":{}}"#;

/// The least lock of the packaging tool `kind`, for a double that serves a
/// project's lock.
pub fn lock(kind: LockKind) -> &'static str {
    match kind {
        LockKind::Uv => UV_LOCK,
        LockKind::Renv => RENV_LOCK,
    }
}

/// The host-functions port's empty double. Like the empty port it keeps no
/// project functions: a build that calls one gets D110's refusal before
/// anything enters the port, and a specification that calls none never asks
/// it anything, so its other methods are never reached.
pub struct NoFunctions;

impl HostFunctions for NoFunctions {
    type Handle = ();
    type Error = Infallible;
    fn is_empty(&self) -> bool {
        true
    }
    fn verify_lock(
        &mut self,
        _: Language,
        _: Lock<'_>,
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

/// A resolver that serves inputs authored bytes and records every capture.
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

impl Resolver for Sources {
    type Error = Infallible;
    fn inspect(&mut self, name: Name<'_>) -> Result<Located, Infallible> {
        let Name::Input(source) = name else {
            panic!("the test serves inputs only, not {name:?}")
        };
        Ok(Located {
            identity: source.path.clone(),
            display_path: source.path.clone(),
        })
    }
    fn capture(&mut self, name: Name<'_>) -> Result<Captured, Infallible> {
        let Name::Input(source) = name else {
            panic!("the test serves inputs only, not {name:?}")
        };
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
        Ok(Captured::resource(Arc::clone(bytes), true))
    }
    fn failure(&self, _: Name<'_>, error: &Infallible) -> Option<Failure> {
        match *error {}
    }
    fn rebase(&mut self, _: &Located, _: &Located, _: &str) -> Result<String, Infallible> {
        panic!("the test serves no layers to rebase")
    }
}
