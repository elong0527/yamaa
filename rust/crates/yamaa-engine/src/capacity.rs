//! Bounded application capacity proposal. These are implementation policies,
//! not language limits or allocator/process-memory guarantees.

/// Unapproved capped source/output/work frontier policy. Graph row reservations
/// keep their separate cumulative 1,048,576-row ceiling.
pub const SOURCE_ROWS: usize = 524_288;
pub const SOURCE_CELLS: usize = 2_097_152;
pub const SOURCE_BYTES: usize = 16 * 1024 * 1024;
pub const EXECUTION_WORK: usize = 128 * 1024 * 1024;
pub const GRAPH_OUTPUT_CELLS: usize = 2_097_152;
pub const OUTPUT_ROWS: usize = 131_072;
pub const OUTPUT_CELLS: usize = 1_048_576;
pub const KEY_CELLS: usize = 524_288;
pub const IDENTITY_CELLS: usize = 65_536;
pub const OUTPUT_TEXT_BYTES: usize = 2 * 1024 * 1024;
pub const IDENTITY_TEXT_BYTES: usize = 1024 * 1024;

/// Unapproved larger frontier candidates. Encoded complete prefixes and actual
/// retained preallocation charges have separate enforced ceilings. Native graph
/// reports pool actual encoded lengths and cumulative retained charges. The
/// 100k authored producer row/CSV geometry bounds the initial encoded probe;
/// measured complete prefixes and requested buffers determine final proposals.
pub const REPORT_ENCODED_BYTES: usize = 160 * 1024 * 1024;
pub const REPORT_RETAINED_FACTOR: usize = 6;
pub const REPORT_RETAINED_BYTES: usize = 768 * 1024 * 1024;

/// Independently bounded report traversal/row counts; never byte or CPU units.
pub const REPORT_CELLS: usize = 4_194_304;
pub const REPORT_WORK: usize = 512 * 1024 * 1024;
