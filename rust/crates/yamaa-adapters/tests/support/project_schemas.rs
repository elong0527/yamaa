use std::sync::Arc;
use yamaa_adapters::specification_source::CapturedSchema;

pub fn environment_schema() -> Arc<CapturedSchema> {
    yamaa_adapters::shipped_schema::capture_environment().unwrap()
}
pub fn domain_schema() -> Arc<CapturedSchema> {
    yamaa_adapters::shipped_schema::capture().unwrap()
}
