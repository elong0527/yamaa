//! Registered native file handles; R performs no preparation or source callbacks.
use crate::specification_service::boundary;
use extendr_api::prelude::*;

#[cfg(unix)]
mod platform {
    use super::*;
    use crate::specification_service::address;
    use std::{
        cell::{Cell, RefCell},
        collections::BTreeMap,
        rc::{Rc, Weak},
    };
    use yamaa_adapters::{file_preparation::FileSpecification, file_resources::Resources};
    thread_local! { static HANDLES: RefCell<BTreeMap<usize, Weak<RefCell<FileSpecification>>>> = const { RefCell::new(BTreeMap::new()) }; }
    struct Handle {
        state: Rc<RefCell<FileSpecification>>,
        identity: Cell<usize>,
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            HANDLES.with(|handles| {
                handles.borrow_mut().remove(&self.identity.get());
            });
        }
    }
    fn text(bytes: &Raw) -> std::result::Result<&str, String> {
        if bytes.len() > 65536 {
            return Err("file specification path byte limit".into());
        }
        std::str::from_utf8(bytes.as_slice())
            .map_err(|_| "invalid file specification path UTF-8".into())
    }
    fn resolve(value: &Robj) -> std::result::Result<Rc<RefCell<FileSpecification>>, String> {
        let id = address(value)?;
        HANDLES
            .with(|handles| handles.borrow().get(&id).and_then(Weak::upgrade))
            .ok_or_else(|| "unknown file specification handle".into())
    }
    pub(super) fn prepare(
        root: Raw,
        base: Raw,
        entry: Raw,
        data_roots: List,
    ) -> std::result::Result<Robj, String> {
        if data_roots.len() >= 64 {
            return Err("approved resource root limit".into());
        }
        let roots = data_roots
            .iter()
            .map(|(_, value)| {
                let raw = value.as_raw().ok_or("resource roots must be raw UTF-8")?;
                text(&raw).map(str::to_owned)
            })
            .collect::<std::result::Result<Vec<_>, String>>()?;
        let resources =
            Resources::new(text(&root)?, text(&base)?, &roots).map_err(|error| error.message())?;
        let state = FileSpecification::prepare(resources, text(&entry)?)
            .map_err(|error| error.into_message())?;
        let handle = ExternalPtr::new(Handle {
            state: Rc::new(RefCell::new(state)),
            identity: Cell::new(0),
        });
        let id = address(handle.as_robj())?;
        handle.identity.set(id);
        HANDLES.with(|handles| {
            handles
                .borrow_mut()
                .insert(id, Rc::downgrade(&handle.state));
        });
        Ok(handle.into_robj())
    }
    pub(super) fn source(handle: Robj) -> std::result::Result<Robj, String> {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "file specification handle is already borrowed")?;
        let source = state.run().source();
        Ok(list!(name = source.name.as_str(), path = source.path.as_str()).into_robj())
    }
    pub(super) fn reads(handle: Robj) -> std::result::Result<Robj, String> {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "file specification handle is already borrowed")?;
        Ok(r!(state.capture_reads() as i32))
    }
    pub(super) fn check(handle: Robj) -> std::result::Result<Robj, String> {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "file specification handle is already borrowed")?;
        yamaa_adapters::specification_check::issues(state.run())
            .map(|issues| r!(issues))
            .map_err(|error| error.message().into())
    }
    pub(super) fn build(handle: Robj, metadata: List) -> std::result::Result<Robj, String> {
        use crate::specification_result::{fields, identity, store};
        let fields = fields(&metadata)?;
        let state = resolve(&handle)?;
        let mut state = state
            .try_borrow_mut()
            .map_err(|_| "file specification handle is already borrowed")?;
        let attempt = state.build();
        if let Some(error) = yamaa_adapters::file_preparation::opaque_resource_failure(&attempt) {
            return Err(error.message().into());
        }
        store(
            yamaa_adapters::specification_report::build_result(
                state.run(),
                &attempt,
                identity(&fields),
            )
            .map_err(|_| "unsupported or invalid build observation")?,
        )
    }
}
#[extendr]
fn prepare_file_specification(root: Raw, base: Raw, entry: Raw, data_roots: List) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::prepare(root, base, entry, data_roots)
        }
        #[cfg(not(unix))]
        {
            let _ = (root, base, entry, data_roots);
            Err("native file preparation unavailable on this platform".into())
        }
    })
}
#[extendr]
fn file_specification_source(handle: Robj) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::source(handle)
        }
        #[cfg(not(unix))]
        {
            let _ = handle;
            Err("native file preparation unavailable on this platform".into())
        }
    })
}
#[extendr]
fn file_specification_reads(handle: Robj) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::reads(handle)
        }
        #[cfg(not(unix))]
        {
            let _ = handle;
            Err("native file preparation unavailable on this platform".into())
        }
    })
}
#[extendr]
fn file_specification_check(handle: Robj) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::check(handle)
        }
        #[cfg(not(unix))]
        {
            let _ = handle;
            Err("native file preparation unavailable on this platform".into())
        }
    })
}
#[extendr]
fn file_specification_build(handle: Robj, metadata: List) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::build(handle, metadata)
        }
        #[cfg(not(unix))]
        {
            let _ = (handle, metadata);
            Err("native file preparation unavailable on this platform".into())
        }
    })
}
extendr_module! { mod file_specification; fn prepare_file_specification; fn file_specification_source; fn file_specification_reads; fn file_specification_check; fn file_specification_build; }
