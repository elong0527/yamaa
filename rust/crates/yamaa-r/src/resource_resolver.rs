//! Root resolver closures on the calling R thread; no R object is sent to workers.
use extendr_api::prelude::*;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread::ThreadId,
};
use yamaa_adapters::host_resources::{Failure, Reply, Request, Resolver};
thread_local! { static CALLBACKS: RefCell<BTreeMap<usize, Function>> = const { RefCell::new(BTreeMap::new()) }; }
static NEXT: AtomicUsize = AtomicUsize::new(1);
struct Port {
    id: usize,
    thread: ThreadId,
}
impl Drop for Port {
    fn drop(&mut self) {
        if self.thread == std::thread::current().id() {
            CALLBACKS.with(|callbacks| callbacks.borrow_mut().remove(&self.id));
        }
    }
}
pub fn new(callback: Function) -> Arc<dyn Resolver> {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    CALLBACKS.with(|callbacks| callbacks.borrow_mut().insert(id, callback));
    Arc::new(Port {
        id,
        thread: std::thread::current().id(),
    })
}
impl Resolver for Port {
    fn request(&self, request: Request<'_>) -> Result<Reply, Failure> {
        if self.thread != std::thread::current().id() {
            return Err(Failure::Rejected);
        }
        let callback = CALLBACKS
            .with(|callbacks| callbacks.borrow().get(&self.id).cloned())
            .ok_or(Failure::Rejected)?;
        let args = list!(
            operation = request.operation.name(),
            kind = request.kind.name(),
            declaring = Raw::from_bytes(request.declaring.as_bytes()),
            written = Raw::from_bytes(request.written.as_bytes()),
            reference = request.reference.map(|r| Raw::from_bytes(r.as_bytes())),
            data = request.bytes.map(Raw::from_bytes)
        );
        // The installed R dispatch helper catches conditions/interrupts and
        // returns closed envelopes; no R longjmp crosses the native boundary.
        let reply = callback
            .call(pairlist!(request = args))
            .map_err(|_| Failure::Rejected)?
            .as_list()
            .ok_or(Failure::Rejected)?;
        let failure = reply.elt(0).map_err(|_| Failure::Rejected)?;
        if !failure.is_null() {
            return Err(Failure::from_name(
                failure.as_str().ok_or(Failure::Rejected)?,
            ));
        }
        let reference = reply.elt(1).map_err(|_| Failure::Rejected)?;
        let reference = reference.as_raw().ok_or(Failure::Rejected)?;
        let reference = std::str::from_utf8(reference.as_slice())
            .map_err(|_| Failure::Rejected)?
            .to_owned();
        let bytes = reply.elt(2).map_err(|_| Failure::Rejected)?;
        let bytes = if bytes.is_null() {
            None
        } else {
            Some(Arc::from(
                bytes.as_raw().ok_or(Failure::Rejected)?.as_slice(),
            ))
        };
        let frame = reply.elt(3).map_err(|_| Failure::Rejected)?;
        let frame = if frame.is_null() {
            None
        } else {
            Some(Arc::new(
                crate::arrow_bridge::import(frame).map_err(|_| Failure::Rejected)?,
            ))
        };
        Ok(Reply {
            reference,
            bytes,
            frame,
        })
    }
}
