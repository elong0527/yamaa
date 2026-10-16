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
                let token = staged
                    .as_ref()
                    .ok()
                    .filter(|reply| {
                        !reply.reference.is_empty()
                            && reply.bytes.is_none()
                            && reply.frame.is_none()
                    })
                    .map_or(reference.as_str(), |reply| reply.reference.as_str());
                let result = match &staged {
                    Ok(reply)
                        if !reply.reference.is_empty()
                            && reply.bytes.is_none()
                            && reply.frame.is_none() =>
                    {
                        resolver
                            .request(request(Operation::Commit, token, None))
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
                    Ok(_) => Err(Failure::Rejected),
                    Err(failure) => Err(*failure),
                };
                if let Err(failure) = result {
                    if resolver
                        .request(request(Operation::Abort, token, None))
                        .is_err()
                    {
                        return Err(file::Error::Host(Failure::Cleanup));
                    }
                    return Err(file::Error::Host(failure));
                }
                Ok(())
            }
        }
    }
}
