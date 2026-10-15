//! The actual ordinary-project attempt retained by a public domain.
pub(super) mod platform {
    use std::sync::Arc;
    type NativeAttempt = yamaa_adapters::project_run::Attempt<
        yamaa_adapters::file_resources::Error,
        crate::project_activation::Error,
    >;
    pub(crate) struct HeldAttempt {
        pub(crate) inner: NativeAttempt,
        pub(crate) run: Arc<yamaa_adapters::project_run::PreparedRun>,
    }
}
