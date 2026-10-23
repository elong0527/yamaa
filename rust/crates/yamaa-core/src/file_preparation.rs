//! Entry preparation and success-only alias acceptance through the resolver
//! port. Physical identities, snapshots, path policy and resource limits
//! belong to the resolver.
use crate::{diagnostic::Diagnostic, resource::ResourceFailure};
use crate::{
    project_preparation::Source,
    resolver::{Captured, Failure, File, Located, Name, Resolver},
};
use std::collections::BTreeMap;

/// The exact immutable retained first-capture catalog, not a replayed iterator.
pub trait PreparedView {
    type Parent;
    fn parents(&self) -> &[Self::Parent];
    fn parent_source(parent: &Self::Parent) -> &Source;
    fn resources(&self) -> impl Iterator<Item = &Source> {
        std::iter::empty()
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
    fn prepare<P: Resolver<Error = E>>(
        &mut self,
        source: Source,
        display_path: String,
        resolver: &mut P,
    ) -> Result<Self::Prepared, Self::Error>;
}
#[derive(Debug, thiserror::Error)]
pub enum Error<E, X> {
    /// The file `written` names could not be resolved or captured as the
    /// entry itself; a failure of any file it declares is `Resource`.
    #[error("file preparation entry: {0:?}")]
    Entry(E),
    #[error("file preparation resource: {0:?}")]
    Resource(E),
    #[error("file preparation preparation: {0:?}")]
    Preparation(X),
    #[error("file preparation resource finding: error={error:?}, diagnostic={diagnostic:?}")]
    ResourceFinding {
        error: E,
        diagnostic: Box<Diagnostic>,
    },
    #[error("file preparation preparation finding: error={error:?}, diagnostic={diagnostic:?}")]
    PreparationFinding {
        error: X,
        diagnostic: Box<Diagnostic>,
    },
}
/// The resolver a document prepares through: it keeps every spelling that
/// located a metadata file, so a located file is captured by the name that
/// located it, and every spelling is captured again once the document
/// prepares.
struct AliasCache<'a, P> {
    port: &'a mut P,
    paths: BTreeMap<String, Vec<(String, String)>>,
    /// The finding the port classified for the last failed parent request or
    /// layer rebase; a failed preparation reports it beside its own error.
    parent_failure: Option<Diagnostic>,
}
impl<P: Resolver> AliasCache<'_, P> {
    /// A `parent` the port refused as written (FILE-5), found to reach a
    /// symbolic link (FILE-8) or found changed (FILE-10) fails with that
    /// condition at `parent`, naming the path as written; any other refusal
    /// stays the port's own error.
    fn parent_error(
        port: &P,
        name: Name<'_>,
        written: &str,
        error: &P::Error,
    ) -> Option<Diagnostic> {
        match port.failure(name, error)?.resource()? {
            failure @ (ResourceFailure::UriScheme
            | ResourceFailure::Backslash
            | ResourceFailure::EmptySegment
            | ResourceFailure::Symlink
            | ResourceFailure::ContentChanged) => Some(failure.field_diagnostic("parent", written)),
            _ => None,
        }
    }
    fn remember(&mut self, source: &Located, declaring: &str, written: &str) {
        let present = self
            .paths
            .get(&source.identity)
            .is_some_and(|paths| paths.iter().any(|(d, w)| d == declaring && w == written));
        if !present {
            self.paths
                .entry(source.identity.clone())
                .or_default()
                .push((declaring.into(), written.into()));
        }
    }
}
impl<P: Resolver> Resolver for AliasCache<'_, P> {
    type Error = P::Error;
    fn inspect(&mut self, name: Name<'_>) -> Result<Located, P::Error> {
        let located = self.port.inspect(name);
        if let Name::Written {
            file,
            declaring,
            written,
        } = name
        {
            match &located {
                Ok(source) => self.remember(source, declaring, written),
                Err(error) if file == File::Parent => {
                    self.parent_failure = Self::parent_error(self.port, name, written, error);
                }
                Err(_) => {}
            }
        }
        located
    }
    /// A located file is captured by the first spelling that located it; one
    /// this cache never located goes to the port as it is.
    fn capture(&mut self, name: Name<'_>) -> Result<Captured, P::Error> {
        let Name::Located { file, source } = name else {
            return self.port.capture(name);
        };
        let Some((declaring, written)) = self
            .paths
            .get(&source.identity)
            .and_then(|paths| paths.first())
            .cloned()
        else {
            return self.port.capture(name);
        };
        let written_name = Name::Written {
            file,
            declaring: &declaring,
            written: &written,
        };
        let captured = self.port.capture(written_name);
        if let (File::Parent, Err(error)) = (file, &captured) {
            self.parent_failure = Self::parent_error(self.port, written_name, &written, error);
        }
        captured
    }
    fn failure(&self, name: Name<'_>, error: &P::Error) -> Option<Failure> {
        self.port.failure(name, error)
    }
    fn is_interrupt(&self, error: &P::Error) -> bool {
        self.port.is_interrupt(error)
    }
    fn rebase(
        &mut self,
        layer: &Located,
        entry: &Located,
        written: &str,
    ) -> Result<String, P::Error> {
        self.port
            .rebase(layer, entry, written)
            .inspect_err(|error| {
                self.parent_failure = self.port.rebase_failure(error, layer, entry, written);
            })
    }
    fn rebase_failure(
        &self,
        error: &P::Error,
        layer: &Located,
        entry: &Located,
        written: &str,
    ) -> Option<Diagnostic> {
        self.port.rebase_failure(error, layer, entry, written)
    }
}
pub type PreparationResult<P, D> = Result<
    <D as DocumentPort<<P as Resolver>::Error>>::Prepared,
    Error<<P as Resolver>::Error, <D as DocumentPort<<P as Resolver>::Error>>::Error>,
>;

/// Resolve and capture precede schema selection and document preparation.
/// Only semantic success captures every distinct alias again, including its
/// first capture. Errors move unchanged and stop effects. The caller selects
/// the prepared entry as its scope's base.
pub fn prepare<P: Resolver, D: DocumentPort<P::Error>>(
    port: &mut P,
    documents: &mut D,
    declaring: Option<&str>,
    written: &str,
) -> PreparationResult<P, D> {
    let entry = Name::Entry { declaring, written };
    let entry_error = |port: &P, error| {
        if port.failure(entry, &error).and_then(Failure::resource)
            == Some(ResourceFailure::ContentChanged)
        {
            Error::ResourceFinding {
                error,
                diagnostic: Box::new(
                    ResourceFailure::ContentChanged.field_diagnostic("specification", written),
                ),
            }
        } else {
            Error::Entry(error)
        }
    };
    let identity = port
        .inspect(entry)
        .map_err(|error| entry_error(port, error))?
        .identity;
    let bytes = port
        .capture(entry)
        .map_err(|error| entry_error(port, error))?
        .bytes
        .to_vec();
    let source = Source {
        identity: identity.clone(),
        bytes,
    };
    let mut aliases = AliasCache {
        port,
        paths: BTreeMap::new(),
        parent_failure: None,
    };
    let mut document = match documents.prepare(source, identity, &mut aliases) {
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
        let Some(paths) = aliases.paths.get(&source.identity) else {
            // Every file a document holds was located through this cache;
            // one it never located is captured as the port captures it.
            let located = Located {
                identity: source.identity.clone(),
                display_path: source.identity.clone(),
            };
            aliases
                .port
                .capture(Name::Located {
                    file: File::Resource,
                    source: &located,
                })
                .map_err(Error::Resource)?;
            continue;
        };
        for (declaring, written) in paths {
            let name = Name::Written {
                file: File::Resource,
                declaring,
                written,
            };
            if let Err(error) = aliases.port.capture(name) {
                if let Some(diagnostic) = aliases
                    .port
                    .failure(name, &error)
                    .and_then(Failure::resource)
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
    document.retain_resource_aliases(aliases.paths);
    Ok(document)
}
