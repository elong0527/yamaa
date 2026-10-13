//! Physical metadata codec port for the native study-inventory application.
use crate::{
    file_preparation,
    file_resources::{MetadataLimits, Resources},
    project_source::{CapturedEnvironment, OwnedEnvironment},
    specification_source::{CapturedSchema, PreparedDocument},
};
use std::sync::Arc;
use yamaa_engine::submission_inventory as service;
struct Native<'a> {
    resources: &'a mut Resources,
    schema: Arc<CapturedSchema>,
}
impl service::Ports for Native<'_> {
    type Specification = PreparedDocument;
    type Error = file_preparation::Error;
    fn capture(&mut self, declaring: &str, written: &str) -> Result<PreparedDocument, Self::Error> {
        file_preparation::prepare_document_from(
            self.resources,
            Some(declaring),
            written,
            Some(Arc::clone(&self.schema)),
        )
    }
    fn document(specification: &PreparedDocument) -> &yamaa_core::schema::Document {
        specification.model().document()
    }
    fn identity(specification: &PreparedDocument) -> &str {
        &specification.source().identity
    }
}
fn capture(
    resources: &mut Resources,
    schema: &Arc<CapturedSchema>,
    root: &crate::specification_source::CapturedDocument,
) -> Result<service::Captured<PreparedDocument>, file_preparation::Error> {
    let limits = yamaa_engine::define::METADATA_LIMITS;
    resources
        .ensure_metadata_scope(MetadataLimits {
            bytes: limits.bytes,
            snapshots: limits.snapshots,
            text: limits.text,
            work: limits.work,
        })
        .map_err(file_preparation::Error::Resource)?;
    service::capture(
        &root.normalized().document,
        &root.source().identity,
        &mut Native {
            resources,
            schema: Arc::clone(schema),
        },
    )
    .map_err(|error| match error {
        service::Error::Port { error, .. } => error,
        service::Error::Metadata(_) | service::Error::Limit => {
            file_preparation::Error::Resource(crate::file_resources::Error::Limit)
        }
    })
}
pub(crate) fn prepare(
    resources: &mut Resources,
    schema: &Arc<CapturedSchema>,
    environment: &mut OwnedEnvironment,
    requested: bool,
    entry: &str,
) -> Result<(), file_preparation::Error> {
    if !requested || environment.inventory_captured() {
        return Ok(());
    }
    let root = environment.root();
    let captured = capture(resources, schema, root)?;
    // Metadata inheritance selects its own codec entry. Restore the admitted
    // domain owner before ordinary source paths acquire data-read authority.
    resources
        .select_entry_base(entry)
        .map_err(file_preparation::Error::Resource)?;
    environment.attach_inventory(captured);
    Ok(())
}
pub(crate) fn prepare_parts(
    resources: &mut Resources,
    schema: &Arc<CapturedSchema>,
    environment: &mut yamaa_core::project_environment::ExecutionEnvironment,
    provenance: &mut CapturedEnvironment,
    requested: bool,
    entry: &str,
) -> Result<(), file_preparation::Error> {
    if !requested || provenance.inventory_captured() {
        return Ok(());
    }
    let root = provenance.root();
    let captured = capture(resources, schema, root)?;
    // Metadata inheritance selects its own codec entry. Restore the admitted
    // domain owner before ordinary source paths acquire data-read authority.
    resources
        .select_entry_base(entry)
        .map_err(file_preparation::Error::Resource)?;
    environment.attach_inventory(captured.inventory);
    provenance.attach_inventory(captured.specifications);
    Ok(())
}
