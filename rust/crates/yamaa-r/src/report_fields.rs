//! Borrow report identity fields already captured by the public request.
use yamaa_adapters::specification_report::Identity;

pub(super) fn identity(fields: &[String]) -> Identity<'_> {
    Identity {
        runtime: "r",
        runtime_version: &fields[0],
        engine_version: &fields[1],
        example: &fields[2],
        specification: &fields[3],
        base_directory: &fields[4],
    }
}
