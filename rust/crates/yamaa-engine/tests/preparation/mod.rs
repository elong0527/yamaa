//! Preparation contracts in the yamaa-engine integration suite.

mod dictionary_preparation;
mod document_preparation;
mod file_preparation;
mod inheritance;
mod inheritance_capture;
#[cfg(feature = "test-support")]
mod project_preparation;
mod schema_capture;
mod source_profile_inspection;
mod standalone_preparation;
mod structural_preparation;
