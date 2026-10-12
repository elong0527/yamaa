//! Shared build request and original metadata admission. Study and publication
//! authority enter only after the selected checked capability is complete.
use alloc::string::String;
use yamaa_core::schema::{Document, DocumentNode};

pub struct Request<'a> {
    pub specification: &'a str,
    pub environment: Option<&'a str>,
}

/// Native quota and snapshot observations; these methods neither decode nor
/// activate code, read study data, or publish artifacts.
pub trait MetadataAdmissionPort {
    type Error;
    fn charge_work(&mut self, work: usize) -> Result<(), Self::Error>;
    fn verify_metadata(&mut self) -> Result<(), (String, Self::Error)>;
}

/// Every build YAML resource uses the same native reader and original error
/// carrier. Existing document and environment applications own parsing and
/// dependency traversal; the reader owns physical capture and alias equality.
pub trait ResourceReader:
    MetadataAdmissionPort
    + crate::file_preparation::NativePort<Error = <Self as MetadataAdmissionPort>::Error>
    + crate::project_preparation::CapturePort<Error = <Self as MetadataAdmissionPort>::Error>
{
    /// Root metadata follows the same resolve/capture/copy order as original
    /// file preparation. Relative dependency requests retain their declaring file.
    fn capture_root(
        &mut self,
        written: &str,
        maximum: usize,
    ) -> Result<crate::project_preparation::Source, <Self as MetadataAdmissionPort>::Error> {
        let identity = crate::file_preparation::NativePort::resolve_entry(self, None, written)?;
        let bytes =
            crate::file_preparation::NativePort::capture_entry(self, None, written, maximum)?;
        Ok(crate::project_preparation::Source { identity, bytes })
    }
}
impl<T> ResourceReader for T where
    T: MetadataAdmissionPort
        + crate::file_preparation::NativePort<Error = <T as MetadataAdmissionPort>::Error>
        + crate::project_preparation::CapturePort<Error = <T as MetadataAdmissionPort>::Error>
{
}

pub struct EntryView<'a> {
    /// The retained pre-default arena; defaults cannot invent producer inputs.
    pub original: &'a Document,
    pub model_nodes: usize,
}
#[derive(Debug)]
pub enum SelectionFailure<E> {
    Resource(E),
    Limit(&'static str),
    Snapshot { identity: String, error: E },
}

/// Sealed metadata decision retained beside the exact captured entry. It grants
/// no compilation, activation, study or publication capability on its own.
pub struct EntrySelection {
    producer: bool,
}
impl EntrySelection {
    pub fn is_producer(&self) -> bool {
        self.producer
    }
}

/// Complete admission and whole original-input inspection precede selection.
/// Opaque refusal and changed-snapshot payloads retain their original identity.
pub fn select_entry<P: MetadataAdmissionPort>(
    view: EntryView<'_>,
    model_node_limit: usize,
    port: &mut P,
) -> Result<EntrySelection, SelectionFailure<P::Error>> {
    let document = view.original;
    port.charge_work(document.nodes().len())
        .map_err(SelectionFailure::Resource)?;
    if view.model_nodes > model_node_limit {
        return Err(SelectionFailure::Limit("producer_graph_document_nodes"));
    }
    let mut producer = false;
    if let Some(input) = document.field(document.root(), "input") {
        if let DocumentNode::Mapping(entries) = &document.nodes()[input] {
            for &(_, declaration) in entries {
                producer |=
                    yamaa_core::producer_admission::is_producer_input(document, declaration);
            }
        }
    }
    port.verify_metadata()
        .map_err(|(identity, error)| SelectionFailure::Snapshot { identity, error })?;
    Ok(EntrySelection { producer })
}
