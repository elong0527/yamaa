//! Borrow shared report metadata from the exact prepared capability. Neither
//! view grants study, activation, decode or publication authority.
use crate::specification_source::PreparedDocument;
use yamaa_core::{
    diagnostic::Diagnostic,
    specification::{PreparedSpecification, SourceDeclaration},
};

mod sealed {
    pub trait Sealed {}
    impl Sealed for crate::specification_run::PreparedRun {}
    impl Sealed for crate::project_run::PreparedRun {}
    #[cfg(any(unix, windows))]
    impl Sealed for crate::file_producer_build::NodeReport<'_> {}
    impl<T: super::RunView + ?Sized> Sealed for std::sync::Arc<T> {}
}

/// A coherent view supplied by a prepared-run or complete producer service, or an Arc.
/// External callers cannot mix unrelated source, document and compiler facts.
pub trait RunView: sealed::Sealed {
    fn document(&self) -> &PreparedDocument;
    fn compiled(&self) -> &PreparedSpecification;
    fn check_diagnostics(&self) -> Vec<Diagnostic>;
    fn source(&self) -> &SourceDeclaration {
        self.compiled().source()
    }
}
impl RunView for crate::specification_run::PreparedRun {
    fn document(&self) -> &PreparedDocument {
        self.document()
    }
    fn compiled(&self) -> &PreparedSpecification {
        self.compiled()
    }
    fn check_diagnostics(&self) -> Vec<Diagnostic> {
        self.check_diagnostics()
    }
}
impl RunView for crate::project_run::PreparedRun {
    fn document(&self) -> &PreparedDocument {
        self.document()
    }
    fn compiled(&self) -> &PreparedSpecification {
        self.compiled()
    }
    fn check_diagnostics(&self) -> Vec<Diagnostic> {
        self.check_diagnostics()
    }
}
#[cfg(any(unix, windows))]
impl RunView for crate::file_producer_build::NodeReport<'_> {
    fn document(&self) -> &PreparedDocument {
        self.document()
    }
    fn compiled(&self) -> &PreparedSpecification {
        self.compiled()
    }
    fn check_diagnostics(&self) -> Vec<Diagnostic> {
        self.compiled().verification_declaration_diagnostics()
    }
}
impl<T: RunView + ?Sized> RunView for std::sync::Arc<T> {
    fn document(&self) -> &PreparedDocument {
        self.as_ref().document()
    }
    fn compiled(&self) -> &PreparedSpecification {
        self.as_ref().compiled()
    }
    fn check_diagnostics(&self) -> Vec<Diagnostic> {
        self.as_ref().check_diagnostics()
    }
}
