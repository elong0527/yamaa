//! Fixed logical admission boundaries, independent of available process memory.
use std::num::NonZeroUsize;

/// One shared policy. Callers supply observed sizes, never resource profiles.
pub struct Limits;

/// An observed boundary exceeds the fixed policy; arithmetic overflow has no size.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{resource}: required {actual:?}, limit {limit}")]
pub struct LimitExceeded {
    pub resource: &'static str,
    pub limit: usize,
    pub actual: Option<usize>,
}

impl Limits {
    pub const SPEC_BYTES: usize = 8 * 1024 * 1024;
    pub const SOURCE_BYTES: usize = 10 * 1024 * 1024 * 1024;
    pub const OUTPUT_BYTES: usize = 10 * 1024 * 1024 * 1024;
    pub const TABLE_ROWS: usize = 10_000_000;
    pub const TABLE_CELLS: usize = 640_000_000;
    /// The driver records of one partition of the partitions every host
    /// runs with by default (#3794, D264): the most whole multiples of
    /// 1,024 records whose partition of [`Self::COLUMNS`] columns stays
    /// within [`Self::TABLE_CELLS`], 610 of them. 1,024 was the 1.0 Parquet
    /// writer's row group; the writer holds rows across partitions until its
    /// row group is full, so its row groups need not divide a partition
    /// (D305). Each binding and the command line spell it where they
    /// construct their engine.
    pub const PARTITION_ROWS: usize = 624_640;
    /// The leading declared keys no partition of the default partitions
    /// splits a group of (#3794): two, so that each partition of an ADaM or
    /// SDTM specification, which declares STUDYID and USUBJID first, holds
    /// whole subjects.
    pub const PARTITION_KEYS: usize = 2;
    /// The compiled size, in bytes, of one regular expression's program.
    pub const REGEX_SIZE: usize = 10 * 1024 * 1024;
    /// The bytes of one text value that a verb which can grow text builds:
    /// `str_pad`, `str_decimal`, `str_template`, `str_replace` and
    /// `STR_JOIN` (#4103). 1 MiB is about 32 times the longest SAS
    /// character value, 32,767 bytes.
    pub const TEXT_BYTES: usize = 1024 * 1024;
    pub const FINDINGS: usize = 10_000;

    pub const COLUMNS: usize = 1024;
    pub const KEYS: usize = 64;
    pub const INPUTS: usize = 16;
    pub const SOURCE_OPERANDS: usize = 64;
    pub const PARENT_OCCURRENCES: usize = 8192;

    // Keep recursion guards for decoded syntax and its semantic interpretation.
    pub const YAML_DEPTH: usize = 64;
    pub const SEMANTIC_DEPTH: usize = 128;

    pub fn spec_bytes(actual: usize) -> Result<(), LimitExceeded> {
        check("spec_bytes", Some(actual), Self::SPEC_BYTES)
    }

    pub fn source_bytes(actual: usize) -> Result<(), LimitExceeded> {
        check("source_bytes", Some(actual), Self::SOURCE_BYTES)
    }

    pub fn output_bytes(actual: usize) -> Result<(), LimitExceeded> {
        check("output_bytes", Some(actual), Self::OUTPUT_BYTES)
    }

    /// Admit an output extension before allocating, retaining arithmetic overflow.
    pub fn output_extension(current: usize, additional: usize) -> Result<(), LimitExceeded> {
        check(
            "output_bytes",
            current.checked_add(additional),
            Self::OUTPUT_BYTES,
        )
    }

    /// Admit one text value before it is built; `None` is a length beyond
    /// the platform's, refused without a size.
    pub fn text_bytes(actual: Option<usize>) -> Result<(), LimitExceeded> {
        check("text_bytes", actual, Self::TEXT_BYTES)
    }

    /// Admit one decoded or materialized table, including a proposed next shape.
    pub fn table(rows: usize, columns: usize) -> Result<(), LimitExceeded> {
        Self::columns(columns)?;
        check("table_rows", Some(rows), Self::TABLE_ROWS)?;
        check("table_cells", rows.checked_mul(columns), Self::TABLE_CELLS)
    }

    /// Admit a table of `rows` rows held in consecutive partitions of at most
    /// `partition` rows, each partition as [`Self::table`] admits a table
    /// (D264). The first partition holds the most rows, so it alone decides;
    /// with no partitions the whole table does.
    pub fn partitioned(
        rows: usize,
        partition: Option<NonZeroUsize>,
        columns: usize,
    ) -> Result<(), LimitExceeded> {
        Self::table(
            partition.map_or(rows, |partition| rows.min(partition.get())),
            columns,
        )
    }

    pub fn columns(actual: usize) -> Result<(), LimitExceeded> {
        check("columns", Some(actual), Self::COLUMNS)
    }

    pub fn keys(actual: usize) -> Result<(), LimitExceeded> {
        check("keys", Some(actual), Self::KEYS)
    }

    pub fn inputs(actual: usize) -> Result<(), LimitExceeded> {
        check("inputs", Some(actual), Self::INPUTS)
    }

    pub fn source_operands(actual: usize) -> Result<(), LimitExceeded> {
        check("source_operands", Some(actual), Self::SOURCE_OPERANDS)
    }

    pub fn parent_occurrences(actual: usize) -> Result<(), LimitExceeded> {
        check("parent_occurrences", Some(actual), Self::PARENT_OCCURRENCES)
    }

    pub fn yaml_depth(actual: usize) -> Result<(), LimitExceeded> {
        check("yaml_depth", Some(actual), Self::YAML_DEPTH)
    }

    pub fn semantic_depth(actual: usize) -> Result<(), LimitExceeded> {
        check("semantic_depth", Some(actual), Self::SEMANTIC_DEPTH)
    }
}

fn check(resource: &'static str, actual: Option<usize>, limit: usize) -> Result<(), LimitExceeded> {
    match actual {
        Some(actual) if actual <= limit => Ok(()),
        actual => Err(LimitExceeded {
            resource,
            limit,
            actual,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_size_and_structure_boundaries_admit_the_edge_and_refuse_the_next_value() {
        type Admission = fn(usize) -> Result<(), LimitExceeded>;
        let boundaries: &[(&str, usize, Admission)] = &[
            ("spec_bytes", Limits::SPEC_BYTES, Limits::spec_bytes),
            ("source_bytes", Limits::SOURCE_BYTES, Limits::source_bytes),
            ("output_bytes", Limits::OUTPUT_BYTES, Limits::output_bytes),
            ("text_bytes", Limits::TEXT_BYTES, |actual| {
                Limits::text_bytes(Some(actual))
            }),
            ("columns", Limits::COLUMNS, Limits::columns),
            ("keys", Limits::KEYS, Limits::keys),
            ("inputs", Limits::INPUTS, Limits::inputs),
            (
                "source_operands",
                Limits::SOURCE_OPERANDS,
                Limits::source_operands,
            ),
            (
                "parent_occurrences",
                Limits::PARENT_OCCURRENCES,
                Limits::parent_occurrences,
            ),
            ("yaml_depth", Limits::YAML_DEPTH, Limits::yaml_depth),
            (
                "semantic_depth",
                Limits::SEMANTIC_DEPTH,
                Limits::semantic_depth,
            ),
        ];
        for &(resource, limit, admit) in boundaries {
            assert_eq!(admit(limit - 1), Ok(()), "{resource} below");
            assert_eq!(admit(limit), Ok(()), "{resource} at");
            assert_eq!(
                admit(limit + 1),
                Err(LimitExceeded {
                    resource,
                    limit,
                    actual: Some(limit + 1),
                }),
                "{resource} above"
            );
        }
    }

    #[test]
    fn output_extensions_admit_the_boundary_and_distinguish_overflow() {
        assert_eq!(
            Limits::output_extension(Limits::OUTPUT_BYTES - 1, 1),
            Ok(())
        );
        for (current, additional, actual) in [
            (Limits::OUTPUT_BYTES, 1, Some(Limits::OUTPUT_BYTES + 1)),
            (usize::MAX, 1, None),
        ] {
            assert_eq!(
                Limits::output_extension(current, additional),
                Err(LimitExceeded {
                    resource: "output_bytes",
                    limit: Limits::OUTPUT_BYTES,
                    actual,
                })
            );
        }
    }

    #[test]
    fn table_geometry_is_checked_without_allocating_rows_or_cells() {
        assert_eq!(Limits::table(Limits::TABLE_ROWS - 1, 1), Ok(()));
        assert_eq!(Limits::table(Limits::TABLE_ROWS, 1), Ok(()));
        assert_eq!(
            Limits::table(Limits::TABLE_ROWS + 1, 1),
            Err(LimitExceeded {
                resource: "table_rows",
                limit: Limits::TABLE_ROWS,
                actual: Some(Limits::TABLE_ROWS + 1),
            })
        );
        // At this legal width the next row crosses the cell bound by one cell.
        assert_eq!(Limits::table(3_575_418, 179), Ok(()));
        assert_eq!(Limits::table(10_000_000, 64), Ok(()));
        assert_eq!(
            Limits::table(3_575_419, 179),
            Err(LimitExceeded {
                resource: "table_cells",
                limit: Limits::TABLE_CELLS,
                actual: Some(Limits::TABLE_CELLS + 1),
            })
        );
        assert_eq!(Limits::table(0, Limits::COLUMNS), Ok(()));
        assert_eq!(
            Limits::table(0, Limits::COLUMNS + 1),
            Err(LimitExceeded {
                resource: "columns",
                limit: Limits::COLUMNS,
                actual: Some(Limits::COLUMNS + 1),
            })
        );
    }

    /// A partitioned table is admitted by its first partition, the largest:
    /// 25,000,000 rows in partitions of 1,000,000 fit at 64 columns, as
    /// 1,000,000 x 64 cells do, while the same rows whole do not; a partition
    /// size beyond the table admits the table whole; and a partition is
    /// refused on its own rows or cells, 11,000,000 rows, or 1,000,000 x 1,024
    /// cells.
    #[test]
    fn a_partitioned_table_is_admitted_one_partition_at_a_time() {
        let rows = |rows| NonZeroUsize::new(rows);
        let refused = |resource, limit, actual| {
            Err(LimitExceeded {
                resource,
                limit,
                actual: Some(actual),
            })
        };
        let cases = [
            (
                "partitions of a million",
                25_000_000,
                rows(1_000_000),
                64,
                Ok(()),
            ),
            (
                "the same rows whole",
                25_000_000,
                None,
                64,
                refused("table_rows", Limits::TABLE_ROWS, 25_000_000),
            ),
            ("a partition beyond the table", 5, rows(10), 3, Ok(())),
            (
                "a partition beyond the row limit",
                12_000_000,
                rows(11_000_000),
                1,
                refused("table_rows", Limits::TABLE_ROWS, 11_000_000),
            ),
            (
                "a partition beyond the cell limit",
                20_000_000,
                rows(1_000_000),
                1024,
                refused("table_cells", Limits::TABLE_CELLS, 1_024_000_000),
            ),
            (
                "too many columns",
                1,
                rows(1),
                Limits::COLUMNS + 1,
                refused("columns", Limits::COLUMNS, Limits::COLUMNS + 1),
            ),
        ];
        for (label, total, partition, columns, expected) in cases {
            assert_eq!(
                Limits::partitioned(total, partition, columns),
                expected,
                "{label}"
            );
        }
    }
}
