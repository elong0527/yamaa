//! Native entry preparation and success-only alias acceptance through typed ports.
//! Physical identities, snapshots, path policy and resource limits belong to the port.
use crate::{
    inheritance::{Source as Identity, SourceError},
    inheritance_capture::CapturePort,
    project_preparation::Source,
};
use alloc::{boxed::Box, collections::BTreeMap, string::String, vec::Vec};
use yamaa_core::{diagnostic::Diagnostic, resource::ResourceFailure};

pub trait NativePort {
    type Error;
    fn resolve_entry(
        &mut self,
        declaring: Option<&str>,
        written: &str,
    ) -> Result<String, Self::Error>;
    /// Capture and admit the existing owned preparation copy, without resetting policy.
    fn capture_entry(
        &mut self,
        declaring: Option<&str>,
        written: &str,
        maximum: usize,
    ) -> Result<Vec<u8>, Self::Error>;
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Identity, SourceError<Self::Error>>;
    fn admit_alias_text(&mut self, bytes: usize) -> Result<(), SourceError<Self::Error>>;
    fn capture_parent(
        &mut self,
        declaring: &str,
        written: &str,
        maximum: usize,
    ) -> Result<Vec<u8>, SourceError<Self::Error>>;
    fn canonicalize_resource(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Identity, SourceError<Self::Error>> {
        self.canonicalize(declaring, written)
    }
    fn capture_resource(
        &mut self,
        declaring: &str,
        written: &str,
        maximum: usize,
    ) -> Result<Vec<u8>, SourceError<Self::Error>> {
        self.capture_parent(declaring, written, maximum)
    }
    fn resource_failure(
        &self,
        _error: &Self::Error,
    ) -> Option<yamaa_core::resource::ResourceFailure> {
        None
    }
    fn rebase(
        &mut self,
        layer: &Identity,
        entry: &Identity,
        written: &str,
        maximum: usize,
    ) -> Result<String, Self::Error>;
    /// Verify the spelling against held bytes; this performs no preparation copy.
    fn accept_alias(
        &mut self,
        declaring: &str,
        written: &str,
        maximum: usize,
    ) -> Result<(), Self::Error>;
    fn select_entry_base(&mut self, identity: &str) -> Result<(), Self::Error>;
    fn missing_alias() -> Self::Error;
}
/// The exact immutable retained first-capture catalog, not a replayed iterator.
pub trait PreparedView {
    type Parent;
    fn parents(&self) -> &[Self::Parent];
    fn parent_source(parent: &Self::Parent) -> &Source;
    fn resources(&self) -> impl Iterator<Item = &Source> {
        core::iter::empty()
    }
    /// Project only an original declaring occurrence retained by this document.
    /// Resolved identities are never substituted for its authored field or path.
    fn resource_diagnostic(
        &self,
        _declaring: &str,
        _written: &str,
        _failure: ResourceFailure,
    ) -> Option<Diagnostic> {
        None
    }
    /// Move the already admitted alias ledger to its prepared document owner.
    fn retain_resource_aliases(&mut self, _aliases: BTreeMap<String, Vec<(String, String)>>) {}
}
/// A schema authority connector delegates to the document application. It must
/// not decode, traverse, normalize or retry a semantic failure in this method.
pub trait DocumentPort<E> {
    type Prepared: PreparedView;
    type Error;
    fn prepare<P: CapturePort<Error = E>>(
        &mut self,
        source: Source,
        display_path: String,
        capture: &mut P,
    ) -> Result<Self::Prepared, Self::Error>;
}
#[derive(Debug)]
pub enum Error<E, X> {
    Resource(E),
    Preparation(X),
    ResourceFinding {
        error: E,
        diagnostic: Box<Diagnostic>,
    },
    PreparationFinding {
        error: X,
        diagnostic: Box<Diagnostic>,
    },
}
struct AliasCache<'a, P> {
    port: &'a mut P,
    paths: BTreeMap<String, Vec<(String, String)>>,
    parent_failure: Option<Diagnostic>,
}
impl<P: NativePort> AliasCache<'_, P> {
    fn parent_error(port: &P, written: &str, error: &SourceError<P::Error>) -> Option<Diagnostic> {
        if let SourceError::Raised(error) = error {
            if port.resource_failure(error) == Some(ResourceFailure::ContentChanged) {
                return Some(ResourceFailure::ContentChanged.field_diagnostic("parents", written));
            }
        }
        None
    }
    fn remember(
        &mut self,
        source: &Identity,
        declaring: &str,
        written: &str,
    ) -> Result<(), SourceError<P::Error>> {
        self.port.admit_alias_text(
            source
                .identity
                .len()
                .saturating_add(source.display_path.len()),
        )?;
        let present = self
            .paths
            .get(&source.identity)
            .is_some_and(|paths| paths.iter().any(|(d, w)| d == declaring && w == written));
        if !present {
            self.port.admit_alias_text(
                declaring
                    .len()
                    .saturating_add(written.len())
                    .saturating_add(source.identity.len()),
            )?;
            // All owned ledger facts are copied only after their complete admission.
            self.paths
                .entry(source.identity.clone())
                .or_default()
                .push((declaring.into(), written.into()));
        }
        Ok(())
    }
}
impl<P: NativePort> CapturePort for AliasCache<'_, P> {
    type Error = P::Error;
    fn canonicalize(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Identity, SourceError<Self::Error>> {
        let source = self
            .port
            .canonicalize(declaring, written)
            .inspect_err(|error| {
                self.parent_failure = Self::parent_error(self.port, written, error);
            })?;
        self.remember(&source, declaring, written)?;
        Ok(source)
    }
    fn capture(
        &mut self,
        source: &Identity,
        maximum: usize,
    ) -> Result<Vec<u8>, SourceError<Self::Error>> {
        let (declaring, written) = self
            .paths
            .get(&source.identity)
            .and_then(|paths| paths.first())
            .ok_or_else(|| SourceError::Raised(P::missing_alias()))?;
        let result = self.port.capture_parent(declaring, written, maximum);
        if let Err(error) = &result {
            self.parent_failure = Self::parent_error(self.port, written, error);
        }
        result
    }
    fn canonicalize_resource(
        &mut self,
        declaring: &str,
        written: &str,
    ) -> Result<Identity, SourceError<P::Error>> {
        let source = self.port.canonicalize_resource(declaring, written)?;
        self.remember(&source, declaring, written)?;
        Ok(source)
    }
    fn capture_resource(
        &mut self,
        source: &Identity,
        maximum: usize,
    ) -> Result<Vec<u8>, SourceError<P::Error>> {
        let (declaring, written) = self
            .paths
            .get(&source.identity)
            .and_then(|paths| paths.first())
            .ok_or_else(|| SourceError::Raised(P::missing_alias()))?;
        self.port.capture_resource(declaring, written, maximum)
    }
    fn resource_failure(&self, error: &P::Error) -> Option<yamaa_core::resource::ResourceFailure> {
        self.port.resource_failure(error)
    }
    fn rebase(
        &mut self,
        layer: &Identity,
        entry: &Identity,
        written: &str,
        maximum: usize,
    ) -> Result<String, Self::Error> {
        self.port.rebase(layer, entry, written, maximum)
    }
}
pub type PreparationResult<P, D> = Result<
    <D as DocumentPort<<P as NativePort>::Error>>::Prepared,
    Error<<P as NativePort>::Error, <D as DocumentPort<<P as NativePort>::Error>>::Error>,
>;

/// Resolve/capture/copy precede schema selection and document preparation. Only
/// semantic success replays every distinct alias, including its first capture,
/// before selecting the original entry base. Errors move unchanged and stop effects.
pub fn prepare<P: NativePort, D: DocumentPort<P::Error>>(
    port: &mut P,
    documents: &mut D,
    declaring: Option<&str>,
    written: &str,
    maximum: usize,
) -> PreparationResult<P, D> {
    let entry_error = |port: &P, error| {
        if port.resource_failure(&error) == Some(ResourceFailure::ContentChanged) {
            Error::ResourceFinding {
                error,
                diagnostic: Box::new(
                    ResourceFailure::ContentChanged.field_diagnostic("specification", written),
                ),
            }
        } else {
            Error::Resource(error)
        }
    };
    let identity = port
        .resolve_entry(declaring, written)
        .map_err(|error| entry_error(port, error))?;
    let bytes = port
        .capture_entry(declaring, written, maximum)
        .map_err(|error| entry_error(port, error))?;
    let source = Source {
        identity: identity.clone(),
        bytes,
    };
    let mut aliases = AliasCache {
        port,
        paths: BTreeMap::new(),
        parent_failure: None,
    };
    let mut document = match documents.prepare(source, identity.clone(), &mut aliases) {
        Ok(document) => document,
        Err(error) => {
            return Err(match aliases.parent_failure {
                Some(diagnostic) => Error::PreparationFinding {
                    error,
                    diagnostic: Box::new(diagnostic),
                },
                None => Error::Preparation(error),
            })
        }
    };
    for source in document
        .parents()
        .iter()
        .map(D::Prepared::parent_source)
        .chain(document.resources())
    {
        let paths = aliases
            .paths
            .get(&source.identity)
            .ok_or_else(|| Error::Resource(P::missing_alias()))?;
        for (declaring, written) in paths {
            let result = aliases
                .port
                .accept_alias(declaring, written, source.bytes.len());
            if let Err(error) = result {
                if let Some(diagnostic) = aliases
                    .port
                    .resource_failure(&error)
                    .filter(|failure| *failure == ResourceFailure::ContentChanged)
                    .and_then(|failure| document.resource_diagnostic(declaring, written, failure))
                {
                    return Err(Error::ResourceFinding {
                        error,
                        diagnostic: Box::new(diagnostic),
                    });
                }
                return Err(Error::Resource(error));
            }
        }
    }
    aliases
        .port
        .select_entry_base(&identity)
        .map_err(Error::Resource)?;
    document.retain_resource_aliases(aliases.paths);
    Ok(document)
}
