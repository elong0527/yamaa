//! One publication boundary for physical targets and host-owned objects.
use crate::{
    file_publication as file,
    host_resources::{Failure, Kind, Operation, Request, Resolver},
};
use std::sync::Arc;
pub(crate) enum AnchoredTarget {
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
    pub(crate) fn publisher(&self, declared: &str) -> Result<Publisher, file::Error> {
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
                    })
                    .map_err(file::Error::Host)?;
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
    pub fn publish(&mut self, declared: &str, bytes: &[u8]) -> Result<(), file::Error> {
        match self {
            Self::File(publisher) => publisher.publish(declared, bytes),
            Self::Host {
                resolver,
                declaring,
                written,
                reference,
            } => {
                if written != declared {
                    return Err(file::Error::PathMismatch);
                }
                yamaa_core::limits::Limits::output_bytes(bytes.len())
                    .map_err(file::Error::Limit)?;
                let request = |operation, token, bytes| Request {
                    operation,
                    kind: Kind::Output,
                    declaring,
                    written,
                    reference: Some(token),
                    bytes,
                };
                let staged = resolver.request(request(Operation::Stage, reference, Some(bytes)));
                // Attempt-owned cleanup: only a Stage that returned a token
                // staged provisional output. A failed Stage is atomic by
                // contract and leaves nothing to abort, so Abort uses the
                // returned staging token and never the target reference.
                let token: Option<String> = staged
                    .as_ref()
                    .ok()
                    .filter(|reply| {
                        !reply.reference.is_empty()
                            && reply.bytes.is_none()
                            && reply.frame.is_none()
                    })
                    .map(|reply| reply.reference.clone());
                let result = match (&staged, token.as_deref()) {
                    (Ok(_), Some(staging_token)) => {
                        resolver
                            .request(request(Operation::Commit, staging_token, None))
                            .and_then(|reply| {
                                if reply.reference == *reference
                                    && reply.bytes.is_none()
                                    && reply.frame.is_none()
                                {
                                    Ok(())
                                } else {
                                    Err(Failure::Publication)
                                }
                            })
                    }
                    (Ok(_), None) => Err(Failure::Rejected),
                    (Err(failure), _) => Err(*failure),
                };
                if let Err(failure) = result {
                    if let Some(staging_token) = token.as_deref() {
                        if resolver
                            .request(request(Operation::Abort, staging_token, None))
                            .is_err()
                        {
                            return Err(file::Error::Host(Failure::Cleanup));
                        }
                    }
                    return Err(file::Error::Host(failure));
                }
                Ok(())
            }
        }
    }
}
