//! Fixed logical admission boundaries, independent of available process memory.
//!
//! A table's rows, columns and cells have no fixed maximum (#4773): a table
//! holds what its source or derivation yields, as far as memory allows.

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
    /// The driver records of one partition of the partitions every host
    /// runs with by default (#3794, D264): 610 whole multiples of 1,024
    /// records, which kept a partition of 1,024 columns within the former
    /// 640,000,000-cell bound, and now sizes the working set a partition
    /// holds. 1,024 was the 1.0 Parquet writer's row group; the writer holds
    /// rows across partitions until its row group is full, so its row groups
    /// need not divide a partition (D305). Every client takes it through the
    /// facade's `Partitions::client_default`.
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

    pub const KEYS: usize = 64;
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

    /// Admit more of a source's decoded bytes before holding them, retaining
    /// arithmetic overflow: what a compressed or encoded source grows to as
    /// it is read stays within the source file limit.
    pub fn source_extension(current: usize, additional: usize) -> Result<(), LimitExceeded> {
        check(
            "source_bytes",
            current.checked_add(additional),
            Self::SOURCE_BYTES,
        )
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

    pub fn keys(actual: usize) -> Result<(), LimitExceeded> {
        check("keys", Some(actual), Self::KEYS)
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
            ("keys", Limits::KEYS, Limits::keys),
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
    fn output_and_source_extensions_admit_the_boundary_and_distinguish_overflow() {
        type Extension = fn(usize, usize) -> Result<(), LimitExceeded>;
        let extensions: [(&str, usize, Extension); 2] = [
            (
                "output_bytes",
                Limits::OUTPUT_BYTES,
                Limits::output_extension,
            ),
            (
                "source_bytes",
                Limits::SOURCE_BYTES,
                Limits::source_extension,
            ),
        ];
        for (resource, limit, extend) in extensions {
            assert_eq!(extend(limit - 1, 1), Ok(()), "{resource} at");
            for (current, additional, actual) in
                [(limit, 1, Some(limit + 1)), (usize::MAX, 1, None)]
            {
                assert_eq!(
                    extend(current, additional),
                    Err(LimitExceeded {
                        resource,
                        limit,
                        actual,
                    }),
                    "{resource} past {current}"
                );
            }
        }
    }
}
