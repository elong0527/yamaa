//! Bounded application capacity proposal. These are implementation policies,
//! not language limits or allocator/process-memory guarantees.

/// Proposed driving/donor and owned output capacities. Work allowance remains
/// unchanged until indexed selection and its accounting are qualified.
pub const SOURCE_ROWS: usize = 524_288;
pub const SOURCE_CELLS: usize = 2_097_152;
pub const OUTPUT_ROWS: usize = 131_072;
pub const OUTPUT_CELLS: usize = 1_048_576;
pub const KEY_CELLS: usize = 1_048_576;
pub const IDENTITY_CELLS: usize = 1_048_576;
pub const OUTPUT_TEXT_BYTES: usize = 8 * 1024 * 1024;
pub const IDENTITY_TEXT_BYTES: usize = 8 * 1024 * 1024;

/// Encoded JSON and pre-clone retained storage have different units. The first
/// prototype leaves the existing encoded ceiling intact while admitting at
/// most four retained-storage bytes per encoded-budget byte, capped at 64 MiB.
/// Allocation charges still precede ownership growth; graph node fractions
/// preserve the aggregate ceiling. Actual RSS qualification remains required.
pub const REPORT_ENCODED_BYTES: usize = 16 * 1024 * 1024;
pub const REPORT_RETAINED_FACTOR: usize = 4;
pub const REPORT_RETAINED_BYTES: usize = 64 * 1024 * 1024;
