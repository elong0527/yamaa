//! The study inventory: the specifications a project's sections list.
use crate::{
    file_preparation, file_resources::Resources, project_source::OwnedEnvironment,
    specification_source::PreparedDocument,
};
use yamaa_core::submission_inventory as service;
/// The specifications a project's sections list, each prepared as its own
/// entry from the project file `root`, with the study's dataset names.
fn capture(
    resources: &mut Resources,
    root: &crate::specification_source::CapturedDocument,
    project: &yamaa_core::ast::Project,
) -> Result<service::Captured<PreparedDocument>, file_preparation::Error> {
    let declaring = &root.source().identity;
    service::capture(
        project,
        |written| file_preparation::prepare_document_from(resources, Some(declaring), written),
        PreparedDocument::document,
    )
    .map_err(|error| match error {
        service::Error::Port { error, .. } => error,
        service::Error::Metadata(error) => file_preparation::Error::Inventory(error),
    })
}
pub(crate) fn prepare(
    resources: &mut Resources,
    environment: &mut OwnedEnvironment,
    requested: bool,
    entry: &str,
) -> Result<(), file_preparation::Error> {
    if !requested || environment.inventory_captured() {
        return Ok(());
    }
    let (Some(root), Some(project)) = (environment.root_optional(), environment.project()) else {
        return Ok(());
    };
    let captured = capture(resources, root, project)?;
    // Metadata inheritance selects its own codec entry. Restore the admitted
    // domain owner before ordinary source paths acquire data-read authority.
    resources
        .select_entry_base(entry)
        .map_err(file_preparation::Error::Resource)?;
    environment.attach_inventory(captured);
    Ok(())
}
