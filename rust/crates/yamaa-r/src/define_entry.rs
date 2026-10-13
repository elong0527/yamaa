//! R transports one registered shared Define owner and fresh host issue views.
use crate::specification_service::{address, boundary};
use extendr_api::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
};
#[cfg(any(unix, windows))]
type State = yamaa_adapters::file_define::Define;
#[cfg(not(any(unix, windows)))]
struct State;
struct Handle {
    state: Rc<RefCell<State>>,
    identity: Cell<usize>,
}
thread_local! { static HANDLES: RefCell<BTreeMap<usize, Weak<RefCell<State>>>> = const { RefCell::new(BTreeMap::new()) }; }
impl Drop for Handle {
    fn drop(&mut self) {
        HANDLES.with(|h| {
            h.borrow_mut().remove(&self.identity.get());
        });
    }
}
fn store(state: State) -> std::result::Result<Robj, String> {
    let handle = ExternalPtr::new(Handle {
        state: Rc::new(RefCell::new(state)),
        identity: Cell::new(0),
    });
    let identity = address(handle.as_robj())?;
    handle.identity.set(identity);
    HANDLES.with(|h| {
        h.borrow_mut()
            .insert(identity, Rc::downgrade(&handle.state));
    });
    Ok(handle.into_robj())
}
fn resolve(handle: &Robj) -> std::result::Result<Rc<RefCell<State>>, String> {
    let identity = address(handle)?;
    HANDLES
        .with(|h| h.borrow().get(&identity).and_then(Weak::upgrade))
        .ok_or_else(|| "unknown Define result handle".into())
}
fn text(value: &Raw) -> std::result::Result<&str, String> {
    if value.len() > 65537 * 4 {
        return Err("Define argument byte limit".into());
    }
    std::str::from_utf8(value.as_slice()).map_err(|_| "invalid Define argument UTF-8".into())
}
#[extendr]
fn define_file(environment: Raw, standard: Raw, created: Robj, installed: Raw) -> List {
    boundary(|| {
        let environment = text(&environment)?;
        let standard = text(&standard)?;
        let installed = text(&installed)?;
        let created = if created.is_null() {
            None
        } else {
            Some(
                created
                    .as_raw()
                    .ok_or("created must be raw UTF-8 or NULL")?,
            )
        };
        let created = created.as_ref().map(text).transpose()?;
        #[cfg(any(unix, windows))]
        {
            store(State::prepare(
                environment,
                standard,
                created,
                installed,
                yamaa_engine::host_values::Language::R,
            ))
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (environment, standard, created, installed);
            store(State)
        }
    })
}
#[extendr]
fn define_issues(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "Define result already borrowed")?;
        #[cfg(any(unix, windows))]
        {
            crate::issue_frame::frame(state.issues())
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = state;
            crate::issue_frame::frame(&[])
        }
    })
}
#[extendr]
fn define_artifacts(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "Define result already borrowed")?;
        #[cfg(any(unix, windows))]
        {
            Ok(
                List::from_values(state.artifacts().map(|(path, bytes, state)| {
                    list!(
                        path = path,
                        bytes = Raw::from_bytes(bytes),
                        state = format!("{state:?}")
                    )
                }))
                .into_robj(),
            )
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = state;
            Ok(List::new(0).into_robj())
        }
    })
}
#[extendr]
fn define_save(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let mut state = state
            .try_borrow_mut()
            .map_err(|_| "Define result already borrowed")?;
        #[cfg(any(unix, windows))]
        {
            match state.save() {
                Ok(saved) => Ok(list!(saved = saved, failed = false).into_robj()),
                Err(_) => {
                    let cause = match state.failure() {
                        Some(yamaa_adapters::file_define::Failure::Publication {
                            error, ..
                        }) => error.message(),
                        _ => "cannot save a failed Define result".into(),
                    };
                    Ok(list!(saved = false, failed = true, cause = cause).into_robj())
                }
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = &mut state;
            Ok(list!(
                saved = false,
                failed = true,
                cause = "native file transport unavailable"
            )
            .into_robj())
        }
    })
}
extendr_module! { mod define_entry; fn define_file; fn define_issues; fn define_artifacts; fn define_save; }
