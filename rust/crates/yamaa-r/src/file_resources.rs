//! Private byte-only filesystem transport. Handles are registered, never dereferenced.
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
    use yamaa_adapters::file_resources::{Error, Resources};

    thread_local! { static HANDLES: RefCell<BTreeMap<usize, Weak<RefCell<Resources>>>> = const { RefCell::new(BTreeMap::new()) }; }
    struct Handle {
        resources: Rc<RefCell<Resources>>,
        identity: Cell<usize>,
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            HANDLES.with(|handles| {
                handles.borrow_mut().remove(&self.identity.get());
            });
        }
    }
    fn text(bytes: &Raw) -> std::result::Result<String, String> {
        if bytes.len() > 65536 {
            return Err("resource path byte limit".into());
        }
        std::str::from_utf8(bytes.as_slice())
            .map(str::to_owned)
            .map_err(|_| "invalid resource path UTF-8".into())
    }
    fn resolve(value: &Robj) -> std::result::Result<Rc<RefCell<Resources>>, String> {
        let id = address(value)?;
        HANDLES
            .with(|handles| handles.borrow().get(&id).and_then(Weak::upgrade))
            .ok_or_else(|| "unknown file resource handle".into())
    }
    fn failure(error: Error) -> std::result::Result<Robj, String> {
        let kind = match error {
            Error::Missing => "missing",
            Error::NotRegularFile => "not_regular_file",
            _ => return Err(error.message().into()),
        };
        Ok(list!(kind = kind, message = error.message()).into_robj())
    }
    pub(super) fn initialize(
        root: Raw,
        base: Raw,
        data_roots: List,
    ) -> std::result::Result<Robj, String> {
        if data_roots.len() >= 64 {
            return Err("approved resource root limit".into());
        }
        let root = text(&root)?;
        let base = text(&base)?;
        let data_roots = data_roots
            .iter()
            .map(|(_, value)| text(&value.as_raw().ok_or("resource roots must be raw UTF-8")?))
            .collect::<std::result::Result<Vec<_>, String>>()?;
        let resources =
            Resources::new(&root, &base, &data_roots).map_err(|error| error.message())?;
        let handle = ExternalPtr::new(Handle {
            resources: Rc::new(RefCell::new(resources)),
            identity: Cell::new(0),
        });
        let id = address(handle.as_robj())?;
        handle.identity.set(id);
        HANDLES.with(|handles| {
            handles
                .borrow_mut()
                .insert(id, Rc::downgrade(&handle.resources));
        });
        Ok(handle.into_robj())
    }
    pub(super) fn inspect(handle: Robj, path: Raw) -> std::result::Result<Robj, String> {
        let resources = resolve(&handle)?;
        let resources = resources
            .try_borrow()
            .map_err(|_| "file resource handle is already borrowed")?;
        match resources.inspect(&text(&path)?) {
            Ok(()) => Ok(r!(NULL)),
            Err(error) => failure(error),
        }
    }
    pub(super) fn capture(
        handle: Robj,
        path: Raw,
        maximum: i32,
    ) -> std::result::Result<Robj, String> {
        if maximum < 0 {
            return Err("resource byte ceiling must be nonnegative".into());
        }
        let resources = resolve(&handle)?;
        let mut resources = resources
            .try_borrow_mut()
            .map_err(|_| "file resource handle is already borrowed")?;
        match resources.capture(&text(&path)?, maximum as usize) {
            Ok((bytes, created)) => Ok(list!(Raw::from_bytes(&bytes), created).into_robj()),
            Err(error) => failure(error),
        }
    }
    pub(super) fn reads(handle: Robj) -> std::result::Result<Robj, String> {
        let resources = resolve(&handle)?;
        let resources = resources
            .try_borrow()
            .map_err(|_| "file resource handle is already borrowed")?;
        Ok(r!(resources.capture_reads() as i32))
    }
}
#[extendr]
fn create_file_resources(root: Raw, base: Raw, data_roots: List) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::initialize(root, base, data_roots)
        }
        #[cfg(not(unix))]
        {
            let _ = (root, base, data_roots);
            Err("native file resources unavailable on this platform".into())
        }
    })
}
#[extendr]
fn inspect_file_resource(handle: Robj, path: Raw) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::inspect(handle, path)
        }
        #[cfg(not(unix))]
        {
            let _ = (handle, path);
            Err("native file resources unavailable on this platform".into())
        }
    })
}
#[extendr]
fn capture_file_resource(handle: Robj, path: Raw, maximum: i32) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::capture(handle, path, maximum)
        }
        #[cfg(not(unix))]
        {
            let _ = (handle, path, maximum);
            Err("native file resources unavailable on this platform".into())
        }
    })
}
#[extendr]
fn file_resource_reads(handle: Robj) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::reads(handle)
        }
        #[cfg(not(unix))]
        {
            let _ = handle;
            Err("native file resources unavailable on this platform".into())
        }
    })
}
extendr_module! { mod file_resources; fn create_file_resources; fn inspect_file_resource; fn capture_file_resource; fn file_resource_reads; }
