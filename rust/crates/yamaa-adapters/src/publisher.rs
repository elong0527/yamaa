//! The publisher port (#3030 section 2.10): the target of one file a save
//! writes, and its stage, commit and abort, implemented by the file
//! publisher (Unix and Windows) and by a caller's resolver (a memory
//! resolver or an embedder's bridge). The file resolver anchors each
//! target; the application's save stages every file of an outcome through
//! it, then commits them in order. A file staged a partition at a time
//! (D264) is asked between two partitions whether to go on.
use crate::{
    file_publication as file,
    host_resources::{Failure, Kind, Operation, Request, Resolver},
};
use std::{path::PathBuf, sync::Arc};
pub enum AnchoredTarget {
    File(file::AnchoredTarget),
    Host {
        resolver: Arc<dyn Resolver>,
        declaring: String,
        written: String,
        reference: String,
    },
}
pub enum Publisher {
    File(file::Publisher),
    Host {
        resolver: Arc<dyn Resolver>,
        declaring: String,
        written: String,
        reference: String,
    },
}
impl AnchoredTarget {
    pub fn publisher(&self, declared: &str) -> Result<Publisher, file::Error> {
        match self {
            Self::File(target) => target.publisher(declared).map(Publisher::File),
            Self::Host {
                resolver,
                declaring,
                written,
                reference,
            } => {
                let reply = resolver
                    .request(Request {
                        operation: Operation::Target,
                        kind: Kind::Output,
                        declaring,
                        written,
                        reference: Some(reference),
                        bytes: None,
                        limit: None,
                    })
                    .map_err(target_refusal)?;
                if reply.reference != *reference || (reply.bytes.is_some() || reply.frame.is_some())
                {
                    return Err(file::Error::Host(Failure::Changed));
                }
                Ok(Publisher::Host {
                    resolver: Arc::clone(resolver),
                    declaring: declaring.clone(),
                    written: declared.into(),
                    reference: reference.clone(),
                })
            }
        }
    }
    /// The path a save reports for this target: a file's absolute path with
    /// forward slashes, or the reference a caller's resolver gave its object.
    pub fn path(&self) -> &str {
        match self {
            Self::File(target) => target.path(),
            Self::Host { reference, .. } => reference,
        }
    }
}
/// A caller's resolver's refusal of a target: a kind it reports is the walk's
/// refusal, as the local file resolver's is (FILE-7, FILE-8), and any other
/// failure stays the resolver's own.
fn target_refusal(failure: Failure) -> file::Error {
    failure
        .resource_kind()
        .and_then(|kind| {
            crate::file_resources::resource_failure(&crate::file_resources::Error::from_kind(kind))
        })
        .map_or(file::Error::Host(failure), file::Error::Unreachable)
}
/// A staged file: the file publisher's private temporary file, or the staging
/// token a caller's resolver returned.
pub enum Staging {
    File(file::Staged),
    Host(String),
}
/// What a staged file left behind when it could not be removed.
pub enum Leftover {
    File(PathBuf, std::io::Error),
    Host,
}
impl Publisher {
    pub fn same_target(&self, other: &Self) -> Result<bool, file::Error> {
        match (self, other) {
            (Self::File(left), Self::File(right)) => left.same_target(right),
            (
                Self::Host {
                    resolver: left,
                    reference: l,
                    ..
                },
                Self::Host {
                    resolver: right,
                    reference: r,
                    ..
                },
            ) => Ok(Arc::ptr_eq(left, right) && l == r),
            _ => Ok(false),
        }
    }
    /// Stage and commit one file on its own, as a save of that one file.
    pub fn publish(&mut self, declared: &str, bytes: &[u8]) -> Result<(), file::Error> {
        let mut locks = file::Locks::default();
        let staging = match self.stage(&mut locks, declared, bytes, &[], &mut |_| Ok(())) {
            Ok(staging) => staging,
            Err(Interruption::Failed(error)) => return Err(error),
            Err(Interruption::Cancelled) => unreachable!("a whole file is never interrupted"),
        };
        self.commit(staging)
    }
    fn request<'a>(
        &'a self,
        operation: Operation,
        token: &'a str,
        bytes: Option<&'a [u8]>,
    ) -> Option<(&'a Arc<dyn Resolver>, Request<'a>)> {
        let Self::Host {
            resolver,
            declaring,
            written,
            ..
        } = self
        else {
            return None;
        };
        Some((
            resolver,
            Request {
                operation,
                kind: Kind::Output,
                declaring,
                written,
                reference: Some(token),
                bytes,
                limit: None,
            },
        ))
    }
    /// Stage `bytes` one partition at a time, where `ends` marks each
    /// partition's end (D264), calling `between` with each partition after
    /// the first before it is written. The file publisher appends each
    /// partition to the one staged file it opened, checking the saved-file
    /// bound as the file grows; a caller's resolver stages complete bytes, so
    /// it is asked once every partition has passed. A cancellation or a
    /// failure between two partitions removes the partial staged file.
    pub fn stage(
        &self,
        locks: &mut file::Locks,
        declared: &str,
        bytes: &[u8],
        ends: &[usize],
        between: &mut dyn FnMut(usize) -> Result<(), Interruption>,
    ) -> Result<Staging, Interruption> {
        let failed = Interruption::Failed;
        let pieces = pieces(bytes, ends);
        let (written, reference) = match self {
            Self::File(publisher) if pieces.len() == 1 => {
                return publisher
                    .stage(locks, declared, bytes)
                    .map(Staging::File)
                    .map_err(failed);
            }
            Self::File(publisher) => {
                yamaa_core::limits::Limits::output_bytes(bytes.len())
                    .map_err(|error| failed(file::Error::Limit(error)))?;
                let mut writing = publisher.open(locks, declared).map_err(failed)?;
                for (partition, piece) in pieces.into_iter().enumerate() {
                    if partition > 0 {
                        match between(partition) {
                            Ok(()) => {}
                            // Dropped unfinished, the staged file is removed.
                            Err(Interruption::Cancelled) => return Err(Interruption::Cancelled),
                            Err(Interruption::Failed(error)) => {
                                return Err(failed(writing.fail(error)))
                            }
                        }
                    }
                    if let Err(error) = writing.append(piece) {
                        return Err(failed(writing.fail(error)));
                    }
                }
                return writing.finish().map(Staging::File).map_err(failed);
            }
            Self::Host {
                written, reference, ..
            } => (written, reference),
        };
        if written != declared {
            return Err(failed(file::Error::PathMismatch));
        }
        yamaa_core::limits::Limits::output_bytes(bytes.len())
            .map_err(|error| failed(file::Error::Limit(error)))?;
        for partition in 1..pieces.len() {
            between(partition)?;
        }
        let (resolver, request) = self
            .request(Operation::Stage, reference, Some(bytes))
            .expect("a host publisher");
        let reply = resolver
            .request(request)
            .map_err(|failure| failed(file::Error::Host(failure)))?;
        // Stage is atomic by contract: a failed Stage leaves nothing to abort,
        // and a reply without a staging token staged nothing.
        if reply.reference.is_empty() || reply.bytes.is_some() || reply.frame.is_some() {
            return Err(failed(file::Error::Host(Failure::Rejected)));
        }
        Ok(Staging::Host(reply.reference))
    }
    pub fn commit(&self, staging: Staging) -> Result<(), file::Error> {
        let token = match (self, staging) {
            (Self::File(publisher), Staging::File(staged)) => return publisher.commit(staged),
            (Self::Host { .. }, Staging::Host(token)) => token,
            _ => unreachable!("a staged file belongs to the publisher that staged it"),
        };
        let Self::Host { reference, .. } = self else {
            unreachable!("a host staging token belongs to a host publisher")
        };
        let (resolver, request) = self
            .request(Operation::Commit, &token, None)
            .expect("a host publisher");
        let committed = resolver.request(request).and_then(|reply| {
            if reply.reference == *reference && reply.bytes.is_none() && reply.frame.is_none() {
                Ok(())
            } else {
                Err(Failure::Publication)
            }
        });
        // Attempt-owned cleanup uses the returned staging token, never the
        // target reference.
        committed.map_err(|failure| match self.abort(Staging::Host(token)) {
            Ok(()) => file::Error::Host(failure),
            Err(_) => file::Error::Host(Failure::Cleanup),
        })
    }
    pub fn abort(&self, staging: Staging) -> Result<(), Leftover> {
        match (self, staging) {
            (Self::File(publisher), Staging::File(staged)) => publisher
                .abort(staged)
                .map_err(|(staging, cleanup)| Leftover::File(staging, cleanup)),
            (_, Staging::File(_)) => unreachable!("a staged file belongs to its file publisher"),
            (_, Staging::Host(token)) => {
                let (resolver, request) = self
                    .request(Operation::Abort, &token, None)
                    .expect("a host staging token belongs to a host publisher");
                resolver
                    .request(request)
                    .map(drop)
                    .map_err(|_| Leftover::Host)
            }
        }
    }
}
/// `error`, keeping what a staged file left behind beside it (OUTPUT-8).
pub fn keeping(error: file::Error, leftover: Leftover) -> file::Error {
    match leftover {
        Leftover::File(staging, cleanup) => file::Error::Cleanup {
            operation: Box::new(error),
            staging,
            cleanup,
        },
        Leftover::Host => file::Error::Host(Failure::Cleanup),
    }
}

/// `bytes` cut where `ends` marks each partition's end, in order; whole when
/// `ends` marks no partition, or anything but consecutive pieces of `bytes`.
fn pieces<'b>(bytes: &'b [u8], ends: &[usize]) -> Vec<&'b [u8]> {
    if ends.last() != Some(&bytes.len()) || ends.windows(2).any(|pair| pair[0] > pair[1]) {
        return vec![bytes];
    }
    let mut start = 0;
    ends.iter()
        .map(|&end| {
            let piece = &bytes[start..end];
            start = end;
            piece
        })
        .collect()
}

/// What a save's call between two partitions of a file it stages asks.
pub enum Interruption {
    /// The save was cancelled.
    Cancelled,
    /// The file cannot be staged, as a test injects a failure.
    Failed(file::Error),
}
