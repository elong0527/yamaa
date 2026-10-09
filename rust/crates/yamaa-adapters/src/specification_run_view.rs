//! Borrow shared report metadata from the exact prepared capability. Neither
//! view grants study, activation, decode or publication authority.
use crate::specification_source::PreparedDocument;
use yamaa_core::{
    diagnostic::Diagnostic,
    specification::{PreparedSpecification, SourceDeclaration},
};

pub trait RunView {
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
