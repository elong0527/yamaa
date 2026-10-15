//! Fixed logical admission boundaries, independent of available process memory.

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
    pub const REGEX_STEPS: usize = 1_000_000;
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

    /// Admit one decoded or materialized table, including a proposed next shape.
    pub fn table(rows: usize, columns: usize) -> Result<(), LimitExceeded> {
        Self::columns(columns)?;
        check("table_rows", Some(rows), Self::TABLE_ROWS)?;
        check("table_cells", rows.checked_mul(columns), Self::TABLE_CELLS)
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
}
