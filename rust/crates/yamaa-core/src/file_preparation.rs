//! Entry preparation and success-only alias acceptance through the resolver
//! port, in three steps a caller takes in order: [`capture_entry`], then the
//! document's own preparation through an [`AliasCache`], then
//! [`AliasCache::accept`] once it has prepared. Physical identities,
//! snapshots, path policy and resource limits belong to the resolver.
use crate::{diagnostic::Diagnostic, resource::ResourceFailure};
use crate::{
    project_preparation::Source,
    resolver::{Captured, Failure, File, Located, Name, Resolver},
};
use std::collections::BTreeMap;

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
/// The spellings that located each metadata file a document holds, by the
/// file's identity: the declaring file and the path as written, in the order
/// they first located it.
pub type Aliases = BTreeMap<String, Vec<(String, String)>>;

/// The resolver a document prepares through: it keeps every spelling that
/// located a metadata file, so a located file is captured by the name that
/// located it, and every spelling is captured again once the document
/// prepares.
pub struct AliasCache<'a, P> {
    port: &'a mut P,
    paths: Aliases,
    /// The finding the port classified for the last failed parent request or
    /// layer rebase; a failed preparation reports it beside its own error.
    parent_failure: Option<Diagnostic>,
}
impl<'a, P: Resolver> AliasCache<'a, P> {
    /// A cache over `port` that has located nothing yet.
    pub fn new(port: &'a mut P) -> Self {
        Self {
            port,
            paths: BTreeMap::new(),
            parent_failure: None,
        }
    }
    /// The document's preparation through this cache failed with `error`:
    /// with the finding the port classified for the last failed parent
    /// request or layer rebase, when there is one.
    pub fn refused<X>(&mut self, error: X) -> Error<P::Error, X> {
        match self.parent_failure.take() {
            Some(diagnostic) => Error::PreparationFinding {
                error,
                diagnostic: Box::new(diagnostic),
            },
            None => Error::Preparation(error),
        }
    }
    /// Once the document has prepared, capture again every distinct spelling
    /// that located each file it holds, `held` by identity, including its
    /// first capture, and return the spellings. A file this cache never
    /// located is captured as the port captures it. A spelling found changed
    /// fails with the finding `diagnostic` gives it, from the declaring file
    /// and the path as written, when the document names one (FILE-10); any
    /// other failure moves unchanged and stops later captures.
    pub fn accept<'h, X>(
        self,
        held: impl IntoIterator<Item = &'h str>,
        diagnostic: impl Fn(&str, &str, ResourceFailure) -> Option<Diagnostic>,
    ) -> Result<Aliases, Error<P::Error, X>> {
        for identity in held {
            let Some(paths) = self.paths.get(identity) else {
                // Every file a document holds was located through this cache;
                // one it never located is captured as the port captures it.
                let located = Located {
                    identity: identity.into(),
                    display_path: identity.into(),
                };
                self.port
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
                if let Err(error) = self.port.capture(name) {
                    if let Some(diagnostic) = self
                        .port
                        .failure(name, &error)
                        .and_then(Failure::resource)
                        .filter(|failure| *failure == ResourceFailure::ContentChanged)
                        .and_then(|failure| diagnostic(declaring, written, failure))
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
        Ok(self.paths)
    }

    /// A `parent` the port refused as written (FILE-5), found to reach a
    /// symbolic link (FILE-8) or found changed (FILE-10) fails with that
    /// condition at `parent`, naming the path as written and the layer that
    /// writes it; any other refusal stays the port's own error.
    fn parent_error(port: &P, name: Name<'_>, error: &P::Error) -> Option<Diagnostic> {
        let Name::Written {
            declaring, written, ..
        } = name
        else {
            return None;
        };
        match port.failure(name, error)?.resource()? {
            failure @ (ResourceFailure::UriScheme
            | ResourceFailure::Backslash
            | ResourceFailure::EmptySegment
            | ResourceFailure::Symlink
            | ResourceFailure::ContentChanged) => Some(crate::inheritance::parent_diagnostic(
                failure, declaring, written,
            )),
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
                    self.parent_failure = Self::parent_error(self.port, name, error);
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
            self.parent_failure = Self::parent_error(self.port, written_name, error);
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
/// Resolve and capture the entry `written` names, from `declaring` or the
/// scope's base, before any schema selection or document preparation. A
/// failure of the entry itself is `Entry`; one found changed fails with
/// FILE-10 at `specification`. The caller then prepares the document through
/// an [`AliasCache`] over the same port, accepts its aliases only once it has
/// prepared, and selects the prepared entry as its scope's base.
pub fn capture_entry<P: Resolver, X>(
    port: &mut P,
    declaring: Option<&str>,
    written: &str,
) -> Result<Source, Error<P::Error, X>> {
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
    Ok(Source { identity, bytes })
}
