//! R transports one registered shared Define owner and fresh host issue views.
use crate::handle::{address, boundary};
use extendr_api::prelude::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
};
use yamaa::{
    CancellationToken, DefineBuild, DefineRequest, Engine, EngineOptions, HostLanguage, SaveOutcome,
};
type State = DefineBuild;
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
    std::str::from_utf8(value.as_slice()).map_err(|_| "invalid Define argument UTF-8".into())
}
#[extendr]
fn define_file(environment: Raw, standard: Raw, created: Robj, installed: Raw) -> List {
    boundary(|| {
        let environment = text(&environment)?.to_owned();
        let standard = text(&standard)?.to_owned();
        let installed = text(&installed)?.to_owned();
        let created = if created.is_null() {
            None
        } else {
            Some(
                created
                    .as_raw()
                    .ok_or("created must be raw UTF-8 or NULL")?,
            )
        };
        let created_text = created.as_ref().map(text).transpose()?.map(str::to_owned);
        let mut request = DefineRequest::new(&environment, &standard, &installed);
        if let Some(created) = created_text.as_deref() {
            request = request.with_created(created);
        }
        let version = String::new();
        let engine = Engine::new(EngineOptions::new(HostLanguage::R, "r".into(), version));
        let define = engine
            .define(&request, &CancellationToken::new())
            .map_err(|error| error.to_string())?;
        store(define)
    })
}
#[extendr]
fn define_issues(handle: Robj) -> List {
    boundary(|| {
        let state = resolve(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "Define result already borrowed")?;
        {
            crate::issue_frame::frame(&state.report().diagnostics)
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
        {
            Ok(
                List::from_values(state.artifacts().into_iter().map(|artifact| {
                    list!(
                        path = artifact.path,
                        bytes = Raw::from_bytes(&artifact.bytes),
                        state = artifact.state
                    )
                }))
                .into_robj(),
            )
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
        {
            match state.save() {
                Ok(SaveOutcome::Saved) => Ok(list!(saved = true, failed = false).into_robj()),
                Ok(_) => Ok(list!(saved = false, failed = false).into_robj()),
                Err(_) => {
                    let cause = state
                        .save_error_message()
                        .unwrap_or_else(|| "cannot save a failed Define result".into());
                    Ok(list!(saved = false, failed = true, cause = cause).into_robj())
                }
            }
        }
    })
}
extendr_module! { mod define_entry; fn define_file; fn define_issues; fn define_artifacts; fn define_save; }
