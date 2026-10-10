//! Registered private candidate-project handles. R payloads remain rooted on the
//! calling thread; incoming pointers are looked up without casting a pointee.
use crate::specification_service::boundary;
use extendr_api::prelude::*;
#[cfg(unix)]
pub(super) mod platform {
    use super::*;
    use crate::specification_service::{address, capture_inputs_with_root};
    use std::{
        cell::{Cell, RefCell},
        collections::BTreeMap,
        rc::{Rc, Weak},
        sync::Arc,
    };
    use yamaa_adapters::{
        file_project::{self, FileProject},
        file_resources::Resources,
    };
    type State = Result<FileProject, file_project::Error>;
    type NativeAttempt = yamaa_adapters::project_run::Attempt<
        yamaa_adapters::file_resources::Error,
        crate::project_activation::Error,
    >;
    pub(crate) struct HeldAttempt {
        pub(crate) inner: NativeAttempt,
        pub(crate) run: Arc<yamaa_adapters::project_run::PreparedRun>,
    }
    thread_local! {
        static PROJECTS: RefCell<BTreeMap<usize, Weak<RefCell<State>>>> = const { RefCell::new(BTreeMap::new()) };
        static ATTEMPTS: RefCell<BTreeMap<usize, Weak<HeldAttempt>>> = const { RefCell::new(BTreeMap::new()) };
    }
    struct ProjectHandle {
        state: Rc<RefCell<State>>,
        identity: Cell<usize>,
    }
    struct AttemptHandle {
        state: Rc<HeldAttempt>,
        identity: Cell<usize>,
    }
    impl Drop for ProjectHandle {
        fn drop(&mut self) {
            PROJECTS.with(|handles| {
                handles.borrow_mut().remove(&self.identity.get());
            });
        }
    }
    impl Drop for AttemptHandle {
        fn drop(&mut self) {
            ATTEMPTS.with(|handles| {
                handles.borrow_mut().remove(&self.identity.get());
            });
        }
    }
    fn text(raw: &Raw) -> std::result::Result<&str, String> {
        if raw.len() > 65536 {
            return Err("project path byte limit".into());
        }
        std::str::from_utf8(raw.as_slice()).map_err(|_| "invalid project path UTF-8".into())
    }
    fn project(handle: &Robj) -> std::result::Result<Rc<RefCell<State>>, String> {
        let id = address(handle)?;
        PROJECTS
            .with(|handles| handles.borrow().get(&id).and_then(Weak::upgrade))
            .ok_or_else(|| "unknown project handle".into())
    }
    pub(crate) fn attempt(handle: &Robj) -> std::result::Result<Rc<HeldAttempt>, String> {
        let id = address(handle)?;
        ATTEMPTS
            .with(|handles| handles.borrow().get(&id).and_then(Weak::upgrade))
            .ok_or_else(|| "unknown project attempt handle".into())
    }
    pub(super) fn prepare(
        paths: List,
        roots: List,
        spec_names: List,
        spec_modules: List,
        env_names: List,
        env_modules: List,
    ) -> std::result::Result<Robj, String> {
        if paths.len() != 4 || roots.len() >= 64 {
            return Err("project path or root count limit".into());
        }
        let paths = paths
            .iter()
            .map(|(_, value)| {
                value
                    .as_raw()
                    .ok_or_else(|| "project paths must be raw UTF-8".to_owned())
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let paths = paths
            .iter()
            .map(text)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let roots = roots
            .iter()
            .map(|(_, value)| {
                let raw = value.as_raw().ok_or("project roots must be raw UTF-8")?;
                text(&raw).map(str::to_owned)
            })
            .collect::<std::result::Result<Vec<_>, String>>()?;
        let empty = Raw::from_bytes(&[]);
        let (specification_schema, _) =
            capture_inputs_with_root(&spec_names, &spec_modules, 0, &empty, &empty, "root_class")?;
        let (environment_schema, _) = capture_inputs_with_root(
            &env_names,
            &env_modules,
            0,
            &empty,
            &empty,
            "environment_class",
        )?;
        let resources =
            Resources::new(paths[0], paths[1], &roots).map_err(|error| error.message())?;
        let state = FileProject::prepare(
            resources,
            paths[2],
            paths[3],
            yamaa_core::project_function::Language::R,
            specification_schema,
            environment_schema,
        );
        let handle = ExternalPtr::new(ProjectHandle {
            state: Rc::new(RefCell::new(state)),
            identity: Cell::new(0),
        });
        let id = address(handle.as_robj())?;
        handle.identity.set(id);
        PROJECTS.with(|handles| {
            handles
                .borrow_mut()
                .insert(id, Rc::downgrade(&handle.state));
        });
        Ok(handle.into_robj())
    }
    pub(super) fn status(handle: Robj) -> std::result::Result<Robj, String> {
        let state = project(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "project handle is already borrowed")?;
        Ok(r!(match &*state {
            Ok(_) => "ready",
            Err(file_project::Error::Resource(_)) => "resource_failure",
            Err(file_project::Error::Environment(_)) => "environment_failure",
            Err(file_project::Error::Document(_)) => "document_failure",
            Err(file_project::Error::Compile(_)) => "compile_failure",
        }))
    }
    pub(super) fn reads(handle: Robj) -> std::result::Result<Robj, String> {
        let state = project(&handle)?;
        let state = state
            .try_borrow()
            .map_err(|_| "project handle is already borrowed")?;
        let project = state.as_ref().map_err(|_| "project preparation failed")?;
        Ok(r!(project.capture_reads() as i32))
    }
    pub(super) fn build(
        handle: Robj,
        verify: Function,
        resolve: Function,
    ) -> std::result::Result<Robj, String> {
        let state = project(&handle)?;
        let mut state = state
            .try_borrow_mut()
            .map_err(|_| "project handle is already borrowed")?;
        let project = state.as_mut().map_err(|_| "project preparation failed")?;
        let inner = project.build_with(|run| {
            crate::project_activation::Port::new(
                run.captured_environment()
                    .lock()
                    .map_or(&[], |lock| &lock.source.bytes),
                verify,
                resolve,
            )
        });
        let handle = ExternalPtr::new(AttemptHandle {
            state: Rc::new(HeldAttempt {
                inner,
                run: project.retained_run(),
            }),
            identity: Cell::new(0),
        });
        let id = address(handle.as_robj())?;
        handle.identity.set(id);
        ATTEMPTS.with(|handles| {
            handles
                .borrow_mut()
                .insert(id, Rc::downgrade(&handle.state));
        });
        Ok(handle.into_robj())
    }
    pub(super) fn observations(handle: Robj) -> std::result::Result<Robj, String> {
        let state = attempt(&handle)?;
        observe(&state)
    }
    pub(crate) fn observe(state: &HeldAttempt) -> std::result::Result<Robj, String> {
        let activation = yamaa_adapters::project_activation_observations::activation(
            &state.run,
            &state.inner.activation,
        )
        .map_err(|_| "invalid or over-limit activation observation")?
        .to_string();
        let mut failures = Vec::new();
        yamaa_adapters::project_attempt::visit_host_failures(&state.inner, |stage, error| {
            failures.push(list!(stage = stage, facts = error.facts()));
        });
        Ok(list!(
            engine_succeeded = yamaa_adapters::project_attempt::execution(&state.inner).is_some(),
            activation = Raw::from_bytes(activation.as_bytes()),
            host_failures = List::from_values(failures),
            prepared_sources = List::from_values(
                [
                    state.run.document().source(),
                    state.run.captured_environment().root().source()
                ]
                .into_iter()
                .map(|source| {
                    list!(
                        identity = Raw::from_bytes(source.identity.as_bytes()),
                        bytes = Raw::from_bytes(&source.bytes)
                    )
                })
            )
        )
        .into_robj())
    }
}
#[extendr]
fn prepare_file_project(
    paths: List,
    roots: List,
    spec_names: List,
    spec_modules: List,
    env_names: List,
    env_modules: List,
) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::prepare(
                paths,
                roots,
                spec_names,
                spec_modules,
                env_names,
                env_modules,
            )
        }
        #[cfg(not(unix))]
        {
            let _ = (
                paths,
                roots,
                spec_names,
                spec_modules,
                env_names,
                env_modules,
            );
            Err("native project preparation unavailable on this platform".into())
        }
    })
}
#[extendr]
fn file_project_status(handle: Robj) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::status(handle)
        }
        #[cfg(not(unix))]
        {
            let _ = handle;
            Err("native project preparation unavailable on this platform".into())
        }
    })
}
#[extendr]
fn file_project_reads(handle: Robj) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::reads(handle)
        }
        #[cfg(not(unix))]
        {
            let _ = handle;
            Err("native project preparation unavailable on this platform".into())
        }
    })
}
#[extendr]
fn file_project_build(handle: Robj, verify: Function, resolve: Function) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::build(handle, verify, resolve)
        }
        #[cfg(not(unix))]
        {
            let _ = (handle, verify, resolve);
            Err("native project preparation unavailable on this platform".into())
        }
    })
}
#[extendr]
fn file_project_observations(handle: Robj) -> List {
    boundary(|| {
        #[cfg(unix)]
        {
            platform::observations(handle)
        }
        #[cfg(not(unix))]
        {
            let _ = handle;
            Err("native project preparation unavailable on this platform".into())
        }
    })
}
extendr_module! { mod file_project; fn prepare_file_project; fn file_project_status; fn file_project_reads; fn file_project_build; fn file_project_observations; }
