//! Host resource authority, separate from codecs, credentials and language rules.
//!
//! References are immutable host identities, never content digests. Captures
//! retain their original bytes and compare those bytes on every freshness check.
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Inspect,
    Capture,
    Target,
    Stage,
    Commit,
    Abort,
}
impl Operation {
    pub fn name(self) -> &'static str {
        match self {
            Self::Inspect => "inspect",
            Self::Capture => "capture",
            Self::Target => "target",
            Self::Stage => "stage",
            Self::Commit => "commit",
            Self::Abort => "abort",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Metadata,
    Input,
    Output,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Input => "input",
            Self::Output => "output",
        }
    }
}
/// Only public resource facts cross this boundary. Credentials belong to the
/// resolver closure and must not appear in references or returned errors.
pub struct Request<'a> {
    pub operation: Operation,
    pub kind: Kind,
    pub declaring: &'a str,
    pub written: &'a str,
    /// The held reference on freshness/publication requests; a staging token on
    /// commit and abort. The host must reject a changed reference.
    pub reference: Option<&'a str>,
    pub bytes: Option<&'a [u8]>,
}
#[derive(Clone, Debug)]
pub struct Reply {
    pub frame: Option<Arc<crate::arrow_table::ArrowTable>>,
    pub reference: String,
    /// Present only for Capture. Inspect and Target must never read data.
    pub bytes: Option<Arc<[u8]>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Missing,
    Changed,
    Cancelled,
    Rejected,
    Publication,
    Cleanup,
}
impl Failure {
    /// Translate a host resolver protocol failure name. Both host resolver
    /// ports speak the same closed name set; any other name is `Rejected`.
    pub fn from_name(name: &str) -> Self {
        match name {
            "missing" => Self::Missing,
            "changed" => Self::Changed,
            "cancelled" => Self::Cancelled,
            "publication" => Self::Publication,
            "cleanup" => Self::Cleanup,
            _ => Self::Rejected,
        }
    }
}
/// A resolver may serve local objects, opaque URI identities or warehouse data.
/// Stage creates a private provisional object and returns its staging token;
/// Commit atomically publishes it; Abort removes it after a failed Commit or
/// an interrupted Stage that returned a token. Stage is atomic by contract:
/// a failed Stage must not leave provisional output, so Abort receives only
/// the staging token and never the target reference. Abort is idempotent and
/// token-scoped. Implementations must return closed failures and must not
/// panic. Python and R adapters contain exceptions/conditions, retaining their
/// original objects.
pub trait Resolver: Send + Sync {
    fn request(&self, request: Request<'_>) -> Result<Reply, Failure>;
}
#[derive(Clone)]
struct Snapshot {
    declaring: String,
    written: String,
    kind: Kind,
    reference: String,
    bytes: Arc<[u8]>,
    frame: Option<Arc<crate::arrow_table::ArrowTable>>,
}
pub(crate) struct Resources {
    pub resolver: Arc<dyn Resolver>,
    pub base: String,
    snapshots: Mutex<Vec<Snapshot>>,
}
impl Resources {
    pub fn new(resolver: Arc<dyn Resolver>, _entry: String) -> Self {
        Self {
            resolver,
            base: "/".into(),
            snapshots: Mutex::new(Vec::new()),
        }
    }
    fn call(
        &self,
        operation: Operation,
        kind: Kind,
        declaring: &str,
        written: &str,
        reference: Option<&str>,
    ) -> Result<Reply, Failure> {
        let reply = self.resolver.request(Request {
            operation,
            kind,
            declaring,
            written,
            reference,
            bytes: None,
        })?;
        if reply.reference.is_empty()
            || reply.reference.contains('\0')
            || (operation != Operation::Capture && (reply.bytes.is_some() || reply.frame.is_some()))
        {
            return Err(Failure::Rejected);
        }
        Ok(reply)
    }
    pub fn resolve(&self, declaring: &str, written: &str, kind: Kind) -> Result<String, Failure> {
        self.call(Operation::Inspect, kind, declaring, written, None)?;
        Self::location(declaring, written)
    }
    /// Logical declaring locations remain independent of the backend identity.
    /// A returned URI is retained evidence and is never interpreted as a path.
    pub fn location(declaring: &str, written: &str) -> Result<String, Failure> {
        if yamaa_core::resource::written_path_failure(written).is_some() || written.contains('\0') {
            return Err(Failure::Rejected);
        }
        let mut parts = if written.starts_with('/') {
            Vec::new()
        } else {
            declaring.rsplit_once('/').map_or(Vec::new(), |(base, _)| {
                base.split('/').filter(|s| !s.is_empty()).collect()
            })
        };
        for part in written.split('/') {
            match part {
                "" | "." => (),
                ".." => {
                    parts.pop();
                }
                name => parts.push(name),
            }
        }
        Ok(format!("/{}", parts.join("/")))
    }
    pub fn target(&self, declaring: &str, written: &str) -> Result<String, Failure> {
        self.call(Operation::Target, Kind::Output, declaring, written, None)
            .map(|reply| reply.reference)
    }
    pub fn capture(
        &self,
        declaring: &str,
        written: &str,
        kind: Kind,
        maximum: usize,
    ) -> Result<(Arc<[u8]>, bool), Failure> {
        let old = self
            .snapshots
            .lock()
            .map_err(|_| Failure::Rejected)?
            .iter()
            .find(|s| s.declaring == declaring && s.written == written && s.kind == kind)
            .cloned();
        let reply = self.call(
            Operation::Capture,
            kind,
            declaring,
            written,
            old.as_ref().map(|s| s.reference.as_str()),
        )?;
        if (reply.bytes.is_some() == reply.frame.is_some())
            || (kind != Kind::Input && reply.frame.is_some())
        {
            return Err(Failure::Rejected);
        }
        let bytes = reply.bytes.unwrap_or_else(|| Arc::from([]));
        let same_frame =
            |old: &Option<Arc<crate::arrow_table::ArrowTable>>,
             new: &Option<Arc<crate::arrow_table::ArrowTable>>| {
                use yamaa_core::table::TableAccess;
                match (old, new) {
                    (None, None) => true,
                    (Some(left), Some(right))
                        if left.schema() == right.schema()
                            && left.row_count() == right.row_count() =>
                    {
                        (0..left.row_count()).all(|row| {
                            (0..left.schema().columns().len()).all(|column| {
                                match (left.cell(row, column), right.cell(row, column)) {
                                    (
                                        Ok(yamaa_core::table::ValueRef::Float(left)),
                                        Ok(yamaa_core::table::ValueRef::Float(right)),
                                    ) => left.get().to_bits() == right.get().to_bits(),
                                    (
                                        Ok(yamaa_core::table::ValueRef::Date(left)),
                                        Ok(yamaa_core::table::ValueRef::Date(right)),
                                    ) => {
                                        left.fields() == right.fields()
                                            && left.collected_precision()
                                                == right.collected_precision()
                                    }
                                    (
                                        Ok(yamaa_core::table::ValueRef::DateTime(left)),
                                        Ok(yamaa_core::table::ValueRef::DateTime(right)),
                                    ) => {
                                        left.fields() == right.fields()
                                            && left.collected_precision()
                                                == right.collected_precision()
                                    }
                                    (Ok(left), Ok(right)) => left == right,
                                    _ => false,
                                }
                            })
                        })
                    }
                    _ => false,
                }
            };
        if bytes.len() > maximum {
            return Err(Failure::Rejected);
        }
        if let Some(old) = old {
            if old.reference != reply.reference
                || old.bytes != bytes
                || !same_frame(&old.frame, &reply.frame)
            {
                return Err(Failure::Changed);
            }
            return Ok((old.bytes, false));
        }
        let mut snapshots = self.snapshots.lock().map_err(|_| Failure::Rejected)?;
        if let Some(old) = snapshots
            .iter()
            .find(|s| s.declaring == declaring && s.written == written && s.kind == kind)
        {
            if old.reference != reply.reference
                || old.bytes != bytes
                || !same_frame(&old.frame, &reply.frame)
            {
                return Err(Failure::Changed);
            }
            return Ok((Arc::clone(&old.bytes), false));
        }
        let known = snapshots.iter().find(|s| s.reference == reply.reference);
        if let Some(old) = known {
            if old.bytes != bytes || !same_frame(&old.frame, &reply.frame) {
                return Err(Failure::Changed);
            }
        }
        let created = known.is_none();
        let bytes = known.map_or(bytes, |old| Arc::clone(&old.bytes));
        let frame = known.map_or(reply.frame.clone(), |old| old.frame.clone());
        snapshots.push(Snapshot {
            declaring: declaring.into(),
            written: written.into(),
            kind,
            reference: reply.reference,
            bytes: Arc::clone(&bytes),
            frame,
        });
        Ok((bytes, created))
    }
    pub fn frame(
        &self,
        declaring: &str,
        written: &str,
    ) -> Option<Arc<crate::arrow_table::ArrowTable>> {
        self.snapshots
            .lock()
            .ok()?
            .iter()
            .find(|s| s.declaring == declaring && s.written == written && s.kind == Kind::Input)?
            .frame
            .clone()
    }
    pub fn len(&self) -> usize {
        self.snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
    pub fn sources(&self) -> Vec<(String, Arc<[u8]>)> {
        self.snapshots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|s| s.frame.is_none())
            .map(|s| (s.reference.clone(), Arc::clone(&s.bytes)))
            .collect()
    }
    pub fn verify(&self, count: usize) -> Result<(), (String, Failure)> {
        let snapshots = self
            .snapshots
            .lock()
            .map_err(|_| (String::new(), Failure::Rejected))?
            .clone();
        if count > snapshots.len() {
            return Err((String::new(), Failure::Rejected));
        }
        for snapshot in snapshots.into_iter().take(count) {
            self.capture(
                &snapshot.declaring,
                &snapshot.written,
                snapshot.kind,
                snapshot.bytes.len(),
            )
            .map_err(|failure| (snapshot.reference, failure))?;
        }
        Ok(())
    }
}

/// Deterministic object store for tests and host-owned local/remote adapters.
/// Replacing a value with a new reference models a remote object-version change.
#[derive(Default)]
pub struct MemoryResolver {
    objects: Mutex<BTreeMap<(String, String), Reply>>,
    staged: Mutex<StagedObjects>,
    published: Mutex<BTreeMap<String, Arc<[u8]>>>,
    cancelled: std::sync::atomic::AtomicBool,
    next_stage: std::sync::atomic::AtomicU64,
}
type StagedObjects = BTreeMap<String, (String, Arc<[u8]>)>;
impl MemoryResolver {
    pub fn insert_frame(
        &self,
        declaring: &str,
        written: &str,
        reference: &str,
        frame: Arc<crate::arrow_table::ArrowTable>,
    ) {
        self.objects.lock().unwrap().insert(
            (declaring.into(), written.into()),
            Reply {
                reference: reference.into(),
                bytes: None,
                frame: Some(frame),
            },
        );
    }
    pub fn insert(&self, declaring: &str, written: &str, reference: &str, bytes: Arc<[u8]>) {
        self.objects.lock().unwrap().insert(
            (declaring.into(), written.into()),
            Reply {
                reference: reference.into(),
                bytes: Some(bytes),
                frame: None,
            },
        );
    }
    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn published(&self, reference: &str) -> Option<Arc<[u8]>> {
        self.published.lock().unwrap().get(reference).cloned()
    }
    pub fn staged_count(&self) -> usize {
        self.staged.lock().unwrap().len()
    }
}
impl Resolver for MemoryResolver {
    fn request(&self, request: Request<'_>) -> Result<Reply, Failure> {
        if request.operation == Operation::Abort {
            let reference = request.reference.ok_or(Failure::Rejected)?;
            // Attempt-owned cleanup: delete only the staged token. Never match
            // on the target identity, so overlapping publishes to the same
            // target cannot delete each other's provisional output.
            self.staged
                .lock()
                .map_err(|_| Failure::Cleanup)?
                .retain(|token, _| token != reference);
            return Ok(Reply {
                reference: request.reference.unwrap().into(),
                bytes: None,
                frame: None,
            });
        }
        if self.cancelled.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Failure::Cancelled);
        }
        let empty = |reference: String| Reply {
            reference,
            bytes: None,
            frame: None,
        };
        match request.operation {
            Operation::Inspect | Operation::Capture => {
                let object = self
                    .objects
                    .lock()
                    .map_err(|_| Failure::Rejected)?
                    .get(&(request.declaring.into(), request.written.into()))
                    .cloned()
                    .ok_or(Failure::Missing)?;
                if request
                    .reference
                    .is_some_and(|held| held != object.reference)
                {
                    return Err(Failure::Changed);
                }
                Ok(if request.operation == Operation::Inspect {
                    empty(object.reference)
                } else {
                    object
                })
            }
            Operation::Target => Resources::location(request.declaring, request.written)
                .map(|path| empty(format!("memory:{path}"))),
            Operation::Stage => {
                let reference = request.reference.ok_or(Failure::Rejected)?;
                let bytes = Arc::from(request.bytes.ok_or(Failure::Rejected)?);
                // Unique per-attempt token: overlapping publishes to the same
                // target stage independently, so a late Abort by one attempt
                // can never remove another attempt's provisional output.
                let sequence = self
                    .next_stage
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let token = format!("stage:{sequence}:{reference}");
                let mut staged = self.staged.lock().map_err(|_| Failure::Publication)?;
                if staged.contains_key(&token) {
                    return Err(Failure::Publication);
                }
                staged.insert(token.clone(), (reference.into(), bytes));
                Ok(empty(token))
            }
            Operation::Commit => {
                let token = request.reference.ok_or(Failure::Rejected)?;
                let (target, bytes) = self
                    .staged
                    .lock()
                    .map_err(|_| Failure::Publication)?
                    .remove(token)
                    .ok_or(Failure::Publication)?;
                self.published
                    .lock()
                    .map_err(|_| Failure::Publication)?
                    .insert(target.clone(), bytes);
                Ok(empty(target))
            }
            Operation::Abort => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_from_name_covers_the_closed_protocol_set() {
        assert_eq!(Failure::from_name("missing"), Failure::Missing);
        assert_eq!(Failure::from_name("changed"), Failure::Changed);
        assert_eq!(Failure::from_name("cancelled"), Failure::Cancelled);
        assert_eq!(Failure::from_name("publication"), Failure::Publication);
        assert_eq!(Failure::from_name("cleanup"), Failure::Cleanup);
        assert_eq!(Failure::from_name("anything-else"), Failure::Rejected);
        assert_eq!(Failure::from_name(""), Failure::Rejected);
    }
}
