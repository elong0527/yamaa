//! The two host ports, where a binding author looks (#3766). Beside the
//! scalar values of [`crate::scalar`], which project functions exchange,
//! they hold what a binding implements to serve the engine and every other
//! type the ports' methods take and return.
//!
//! - [`functions`]: the project-function port, [`ProjectFunctions`], with
//!   the signatures it binds, the arguments it is called with, the lock
//!   verification an activation asks of it, and its failures.
//! - [`resolver`]: the resource port, [`ResourceResolver`], with its
//!   requests, replies and failures, and the [`MemoryResolver`] that serves
//!   resources from memory.
//!
//! An embedder looks at the crate root instead, which re-exports only what
//! configures an [`Engine`](crate::Engine): [`ResourceResolver`],
//! [`MemoryResolver`], [`ProjectFunctions`] and [`HostLanguage`].
//!
//! [`ProjectFunctions`]: functions::ProjectFunctions
//! [`HostLanguage`]: functions::HostLanguage
//! [`ResourceResolver`]: resolver::ResourceResolver
//! [`MemoryResolver`]: resolver::MemoryResolver

pub mod functions;
pub mod resolver;
