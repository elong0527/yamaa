use std::sync::Arc;
use yamaa_adapters::{
    project_source::ProjectSchemas,
    specification_source::{CapturedSchema, Source},
};

/// The separately versioned environment root alone.
pub fn environment_schema() -> Arc<CapturedSchema> {
    yamaa_adapters::shipped_schema::capture_environment().unwrap()
}
/// The project root and the environment root a project preparation decodes.
pub fn project_schemas() -> ProjectSchemas {
    yamaa_adapters::shipped_schema::capture_project().unwrap()
}
pub fn domain_schema() -> Arc<CapturedSchema> {
    yamaa_adapters::shipped_schema::capture().unwrap()
}

fn module(identity: &str, text: &str) -> Source {
    Source {
        identity: identity.into(),
        bytes: text.as_bytes().to_vec(),
    }
}
/// The frozen candidate environment root, entry module first. It holds only
/// installed code: schema_version, lock and function definitions.
pub fn candidate_environment_modules() -> Vec<Source> {
    vec![
        module(
            "schema_environment.yaml",
            include_str!("../fixtures/environment-candidate/schema_environment.yaml"),
        ),
        module(
            "schema_shared.yaml",
            include_str!("../fixtures/environment-candidate/schema_shared.yaml"),
        ),
    ]
}
/// The frozen candidate project root, entry module first. It links one
/// environment file per host language and holds codelists, study and sections.
pub fn candidate_project_modules() -> Vec<Source> {
    vec![
        module(
            "schema_project.yaml",
            include_str!("../fixtures/environment-candidate/schema_project.yaml"),
        ),
        module(
            "schema_shared.yaml",
            include_str!("../fixtures/environment-candidate/schema_shared.yaml"),
        ),
    ]
}
/// Both frozen candidate roots, admitted independently of the shipped closures.
pub fn candidate_schemas() -> ProjectSchemas {
    ProjectSchemas {
        project: CapturedSchema::admit_root(candidate_project_modules(), 0, "project_class")
            .unwrap(),
        environment: CapturedSchema::admit_root(
            candidate_environment_modules(),
            0,
            "environment_class",
        )
        .unwrap(),
    }
}
