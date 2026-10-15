//! Approved bounded application capacity for the public longitudinal profile.
//! These are implementation policies, not language limits or process-memory guarantees.

pub const SOURCE_ROWS: usize = 10_000_000;
pub const SOURCE_CELLS: usize = 640_000_000;
pub const SOURCE_BYTES: usize = 1024 * 1024 * 1024;
pub const EXECUTION_WORK: usize = 2_000_000_000;
pub const OUTPUT_ROWS: usize = 10_000_000;
pub const OUTPUT_CELLS: usize = 640_000_000;
pub const KEY_CELLS: usize = 640_000_000;
pub const IDENTITY_CELLS: usize = 640_000_000;
pub const OUTPUT_TEXT_BYTES: usize = 512 * 1024 * 1024;
pub const IDENTITY_TEXT_BYTES: usize = 1024 * 1024 * 1024;

/// Source values charge 32 bytes per cell and row owners charge 24 bytes.
/// Text conversion/ownership has a three-times-source reserve. Compressed
/// decoded text still has to fit this enforced modeled storage allowance.
pub const SOURCE_STORAGE_BYTES: usize =
    32 * SOURCE_CELLS + 24 * SOURCE_ROWS + 3 * SOURCE_BYTES + 64 * 1024 * 1024;
/// Internal public Arrow/Option/IPC projections charge 64 bytes per typed cell.
/// This is separate from the authored output artifact's 1 GiB byte policy.
pub const OUTPUT_STORAGE_BYTES: usize = 64 * OUTPUT_CELLS + OUTPUT_TEXT_BYTES + 64 * 1024 * 1024;

/// A report can visit source cells, derived cells, artifact cells, and three
/// public typed projection passes. These passes share logical traversal quotas.
pub const REPORT_CELLS: usize = SOURCE_CELLS + 3 * OUTPUT_CELLS;
/// Complete report storage derives from its admitted geometry. The 256-byte
/// cell reserve covers type/value maps, scalar spellings and artifact records;
/// 12 bytes per text/artifact byte covers JSON escaping and duplicate records.
/// Rows reserve delimiters/owners, and metadata keeps a separate 64 MiB slack.
pub const REPORT_ENCODED_BYTES: usize = 256 * REPORT_CELLS
    + 12 * (SOURCE_BYTES + OUTPUT_TEXT_BYTES + SOURCE_BYTES)
    + 16 * (SOURCE_ROWS + OUTPUT_ROWS)
    + 64 * 1024 * 1024;
pub const REPORT_RETAINED_FACTOR: usize = 6;
pub const REPORT_RETAINED_BYTES: usize = REPORT_RETAINED_FACTOR * REPORT_ENCODED_BYTES;

/// Entry/cell visits and encoded-buffer writes are separate charged work units.
pub const REPORT_WORK: usize = 8 * REPORT_ENCODED_BYTES + 16 * REPORT_CELLS;
