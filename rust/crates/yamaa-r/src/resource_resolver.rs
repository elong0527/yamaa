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
use yamaa::{ResolverFailure, ResolverReply, ResolverRequest, ResourceResolver};
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
pub fn new(callback: Function) -> Arc<dyn ResourceResolver> {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    CALLBACKS.with(|callbacks| callbacks.borrow_mut().insert(id, callback));
    Arc::new(Port {
        id,
        thread: std::thread::current().id(),
    })
}
impl ResourceResolver for Port {
    fn request(&self, request: ResolverRequest<'_>) -> Result<ResolverReply, ResolverFailure> {
        if self.thread != std::thread::current().id() {
            return Err(ResolverFailure::from_code("rejected"));
        }
        let rejected = || ResolverFailure::from_code("rejected");
        let callback = CALLBACKS
            .with(|callbacks| callbacks.borrow().get(&self.id).cloned())
            .ok_or_else(rejected)?;
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
            .map_err(|_| rejected())?
            .as_list()
            .ok_or_else(rejected)?;
        let failure = reply.elt(0).map_err(|_| rejected())?;
        if !failure.is_null() {
            return Err(ResolverFailure::from_code(
                failure.as_str().ok_or_else(rejected)?,
            ));
        }
        let reference = reply.elt(1).map_err(|_| rejected())?;
        let reference = reference.as_raw().ok_or_else(rejected)?;
        let reference = std::str::from_utf8(reference.as_slice())
            .map_err(|_| rejected())?
            .to_owned();
        let bytes = reply.elt(2).map_err(|_| rejected())?;
        let bytes = if bytes.is_null() {
            None
        } else {
            Some(Arc::from(bytes.as_raw().ok_or_else(rejected)?.as_slice()))
        };
        let frame = reply.elt(3).map_err(|_| rejected())?;
        let frame = if frame.is_null() {
            None
        } else {
            Some(crate::arrow_bridge::import(frame).map_err(|_| rejected())?)
        };
        Ok(ResolverReply::new(reference, bytes, frame))
    }
}
