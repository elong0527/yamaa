//! A 1.0 specification prepared from an authored tree as the 1.0 front end
//! lowers it (#3030 D76): the tree is the normalized document schema
//! admission gives, starting with the `yamaa` and `type` header (D4), and
//! preparation and binding read only the lowered tree.
use super::document::Tree;
use yamaa_core::{
    front_end::v1::spec as front_end,
    producer_contract::Links,
    project_environment::ExecutionEnvironment,
    schema::Document,
    specification::{PrepareError, PreparedSpecification},
};

/// The header fields a 1.0 specification starts with.
pub fn header() -> Vec<(&'static str, Tree)> {
    vec![("yamaa", Tree::text("1.0")), ("type", Tree::text("spec"))]
}

/// Prepare the 1.0 specification `document` without a project file, so each
/// binding only a project supplies fails validation (CT-13).
pub fn prepare(document: &Document) -> Result<PreparedSpecification, PrepareError> {
    prepare_linked(document, None, Links::default())
}

/// Prepare the 1.0 specification `document` against `environment`, the
/// project's admitted functions, codelists and inventory.
#[allow(dead_code)]
pub fn prepare_in(
    document: &Document,
    environment: &ExecutionEnvironment,
) -> Result<PreparedSpecification, PrepareError> {
    prepare_linked(document, Some(environment), Links::default())
}

/// Prepare the 1.0 specification `document` with the producing
/// specifications its `schema` links name.
pub fn prepare_linked(
    document: &Document,
    environment: Option<&ExecutionEnvironment>,
    links: Links<'_>,
) -> Result<PreparedSpecification, PrepareError> {
    let tree = front_end::lower(document, None).into_spec();
    PreparedSpecification::prepare_tree_linked(tree, document, environment, links)
}
