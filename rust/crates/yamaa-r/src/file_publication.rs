//! Private explicit-target publication transport with registered handles.
use crate::specification_service::boundary;
use extendr_api::prelude::*;
#[cfg(unix)]
pub(super) mod platform {
    use super::*;
    use crate::specification_service::address;
    use std::{
        cell::{Cell, RefCell},
        collections::BTreeMap,
        rc::{Rc, Weak},
    };
    use yamaa_adapters::file_publication::Publisher;
    thread_local! { static HANDLES: RefCell<BTreeMap<usize,Weak<RefCell<Publisher>>>> = const { RefCell::new(BTreeMap::new()) }; }
    struct Handle {
        publisher: Rc<RefCell<Publisher>>,
        identity: Cell<usize>,
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            HANDLES.with(|h| {
                h.borrow_mut().remove(&self.identity.get());
            });
        }
    }
    fn text(bytes: &Raw) -> std::result::Result<String, String> {
        if bytes.len() > 65_536 {
            return Err("publication path byte limit".into());
        }
        std::str::from_utf8(bytes.as_slice())
            .map(str::to_owned)
            .map_err(|_| "invalid publication path UTF-8".into())
    }
    fn resolve(handle: &Robj) -> std::result::Result<Rc<RefCell<Publisher>>, String> {
        let id = address(handle)?;
        HANDLES
            .with(|h| h.borrow().get(&id).and_then(Weak::upgrade))
            .ok_or_else(|| "unknown file publication handle".into())
    }
    pub(super) fn save(result: Robj, handle: Robj) -> std::result::Result<Robj, String> {
        let result = crate::specification_result::resolve(&result)?;
        save_result(&result, handle)
    }
    pub(crate) fn save_result(
        result: &yamaa_adapters::specification_report::BuildResult,
        handle: Robj,
    ) -> std::result::Result<Robj, String> {
        let publisher = resolve(&handle)?;
        let mut publisher = publisher
            .try_borrow_mut()
            .map_err(|_| "file publication handle is already borrowed")?;
        result
            .save(&mut *publisher)
            .map(|report| r!(report.to_string()))
            .map_err(|error| match error {
                yamaa_engine::specification_output::SaveError::FailedBuild => {
                    "cannot save a failed build".into()
                }
                yamaa_engine::specification_output::SaveError::Publish(error) => error.message(),
            })
    }
    pub(super) fn create(declared: Raw, target: Raw) -> std::result::Result<Robj, String> {
        let publisher =
            Publisher::new(&text(&declared)?, &text(&target)?).map_err(|e| e.message())?;
        let handle = ExternalPtr::new(Handle {
            publisher: Rc::new(RefCell::new(publisher)),
            identity: Cell::new(0),
        });
        let id = address(handle.as_robj())?;
        handle.identity.set(id);
        HANDLES.with(|h| {
            h.borrow_mut().insert(id, Rc::downgrade(&handle.publisher));
        });
        Ok(handle.into_robj())
    }
    pub(super) fn publish(
        handle: Robj,
        path: Raw,
        content: Raw,
    ) -> std::result::Result<Robj, String> {
        if content.len() > 67_108_864 {
            return Err("publication byte or path limit".into());
        }
        let id = address(&handle)?;
        let publisher = HANDLES
            .with(|h| h.borrow().get(&id).and_then(Weak::upgrade))
            .ok_or("unknown file publication handle")?;
        let mut publisher = publisher
            .try_borrow_mut()
            .map_err(|_| "file publication handle is already borrowed")?;
        publisher
            .publish(&text(&path)?, content.as_slice())
            .map_err(|e| e.message())?;
        Ok(r!(true))
    }
}
#[extendr]
fn create_file_publisher(declared: Raw, target: Raw) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::create(declared, target)
        }
        #[cfg(not(unix))]
        {
            let _ = (declared, target);
            Err("native file publication unavailable on this platform".into())
        }
    })
}
#[extendr]
fn publish_file_artifact(handle: Robj, path: Raw, content: Raw) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::publish(handle, path, content)
        }
        #[cfg(not(unix))]
        {
            let _ = (handle, path, content);
            Err("native file publication unavailable on this platform".into())
        }
    })
}
#[extendr]
fn save_build_file_artifact(result: Robj, publisher: Robj) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::save(result, publisher)
        }
        #[cfg(not(unix))]
        {
            let _ = (result, publisher);
            Err("native file publication unavailable on this platform".into())
        }
    })
}
extendr_module! { mod file_publication; fn create_file_publisher; fn publish_file_artifact; fn save_build_file_artifact; }
