//! A table saved to a file its caller names, as the command line's
//! `read-odm SOURCE OUT` saves the item table `read_odm` returns.
//!
//! - **Profile.** The case-insensitive extension of the path's last segment
//!   selects the `csv` or `parquet` profile from OUTPUT-5's closed mapping;
//!   any other extension, or none, fails as `unknown_artifact_profile` before
//!   anything is written.
//! - **Bytes.** The encoders of a run's output write the table, with no
//!   display precision, so the file holds the bytes a run saves for the same
//!   table under those profiles.
//! - **Publication.** The file is saved as a save of one file through the one
//!   save's publisher: its target is opened, the file is staged beside it,
//!   then committed (OUTPUT-8). Through the local file resolver the path is a
//!   caller path: its parent directory is selected once, and its last segment
//!   is the caller's own file name (FILE-7, FILE-8). Through a caller's
//!   resolver, the path is that resolver's name for the target.
//!
//! The refusals are a run's: a target in a directory that does not exist, a
//! target that is a directory or a link, fails with FILE-7's or FILE-8's
//! condition under `output`, naming the path as given at no field; any other
//! target that cannot be written fails as `engine_rejected` in the `output`
//! stage with the save's code, and a file that cannot be staged or committed
//! also names the path and the attempt, as a specification's file does,
//! without a spec path.

use std::sync::Arc;

use yamaa_core::application_issue::{self as issue, Fault, Step};
use yamaa_core::json::Value;

use crate::{
    column_table::{ColumnTable, TableError},
    file_application::{
        publication_failure, publication_stop, target_failure, unresolved_target, Failure,
    },
    file_publication::Error as PublicationError,
    file_resources::Resources,
    host_resources::Resolver,
    public_table::PublicTable,
    publication::{self, Cause},
    publisher::AnchoredTarget,
};

/// Save `table` to the file `written` names and return the path written:
/// absolute with forward slashes through the local file resolver, or the
/// reference a caller's resolver gave its target (D38, D99). `resolver` is a
/// caller's resolver, or `None` for the local file resolver. A refusal writes
/// nothing.
pub fn save(
    table: &PublicTable,
    written: &str,
    resolver: Option<Arc<dyn Resolver>>,
) -> Result<Vec<String>, Failure> {
    let profile = profile(written).ok_or_else(|| {
        vec![yamaa_core::specification::unknown_artifact_profile(
            written,
            Vec::new(),
        )]
    })?;
    let bytes = encode(table, profile)?;
    let target = target(written, resolver)?;
    let files = [publication::File {
        target: &target,
        written,
        bytes: &bytes,
        partitions: &[],
    }];
    publication::stage(&files)
        .and_then(publication::Staged::commit)
        .map_err(|stop| refusal(stop.cause, written))
}

/// The profile OUTPUT-5's closed mapping gives the file `written` names. A
/// Windows path separates its segments with either separator.
fn profile(written: &str) -> Option<&'static str> {
    if cfg!(windows) {
        yamaa_core::specification::artifact_profile(&written.replace('\\', "/"))
    } else {
        yamaa_core::specification::artifact_profile(written)
    }
}

/// `table`'s bytes under `profile`, from the encoders a run's output uses. A
/// column of a type neither profile writes fails as `output_schema`, naming
/// the column; a bound fails as `output_limit` and any other encoding failure
/// as `output_projection`, as a run's output does. A build without Parquet
/// fails a `parquet` file as `parquet_codec_unavailable`.
fn encode(table: &PublicTable, profile: &str) -> Result<Vec<u8>, Failure> {
    let snapshot =
        ColumnTable::from_public(table).map_err(|error| unrepresentable(table, error))?;
    let projection = (0..table.width()).collect::<Vec<_>>();
    let refused = |fault| vec![issue::rejected(Step::Output, fault)];
    if profile == "csv" {
        crate::csv_artifact::render(&snapshot, &projection, None).map_err(|error| match error {
            crate::csv_artifact::Error::Limit {
                resource,
                limit,
                actual,
                ..
            } => over(resource, limit, actual),
            _ => refused(Fault::OutputProjection),
        })
    } else {
        crate::parquet_artifact::render(&snapshot, &projection).map_err(|error| match error {
            crate::parquet_artifact::Error::Unavailable => refused(Fault::ParquetCodecUnavailable),
            crate::parquet_artifact::Error::Limit {
                resource,
                limit,
                actual,
            } => over(resource, limit, actual),
            _ => refused(Fault::OutputProjection),
        })
    }
}

/// A bound the encoded file exceeds.
fn over(resource: &'static str, limit: usize, actual: Option<usize>) -> Failure {
    vec![issue::resource_rejected(
        Step::Output,
        Fault::OutputLimit,
        resource,
        limit,
        actual,
        Vec::new(),
    )]
}

/// A table no profile can carry, such as an issues table with list columns:
/// `output_schema`, naming the first column of another type.
fn unrepresentable(table: &PublicTable, error: TableError) -> Failure {
    match error {
        TableError::Limit {
            resource,
            limit,
            required,
        } => over(resource, limit, required),
        error => {
            let mut finding = issue::rejected(Step::Output, Fault::OutputSchema);
            if let TableError::ArrayType { column, .. } = error {
                if column < table.width() {
                    let name = Value::String(table.name(column).to_owned());
                    finding.context.insert("column".into(), name);
                }
            }
            vec![finding]
        }
    }
}

/// The target of the file `written` names. Through the local file resolver,
/// a path whose last segment is empty, `.` or `..` names no file, and a name
/// that cannot be one cannot be written: each fails as `invalid_target`. A
/// parent directory that does not exist reaches no entry, and a file in its
/// place stops the walk as not a regular file (FILE-7). Through a caller's
/// resolver, a refused target reply fails as a run's does.
fn target(written: &str, resolver: Option<Arc<dyn Resolver>>) -> Result<AnchoredTarget, Failure> {
    if let Some(resolver) = resolver {
        return Resources::with_resolver(resolver, written)
            .caller_target(written)
            .map_err(|error| unresolved_target(&error, None, written));
    }
    let separators: &[char] = if cfg!(windows) { &['/', '\\'] } else { &['/'] };
    if matches!(
        written.rsplit(separators).next(),
        None | Some("" | "." | "..")
    ) {
        return Err(publication_failure(&PublicationError::InvalidTarget));
    }
    let refused = |error| {
        let cause = crate::file_resources::resource_failure(&error).map_or(
            PublicationError::InvalidTarget,
            PublicationError::Unreachable,
        );
        target_failure(&cause, None, written)
    };
    let (resources, name) = crate::file_configuration::resources(written)
        .map_err(|crate::file_configuration::Error::Path(error)| refused(error))?;
    resources.caller_target(&name).map_err(refused)
}

/// The issues of a save stopped at its one file, as a run's save records a
/// stop: a target refused before the save entered the file, or a file that
/// could not be staged or committed, naming the file and the attempt.
fn refusal(cause: Cause, written: &str) -> Failure {
    let at = |issues| publication_stop(issues, None, written, 1, 0);
    match cause {
        Cause::Target(error) => target_failure(&error, None, written),
        // A save of one file names no earlier target.
        Cause::Collision { .. } => publication_failure(&PublicationError::InvalidTarget),
        Cause::Stage(error) | Cause::Commit(error) => at(target_failure(&error, None, written)),
        // A save of one whole file is never asked between partitions.
        Cause::Cancelled => at(publication_failure(&PublicationError::Host(
            crate::host_resources::Failure::Cancelled,
        ))),
        Cause::Unwind(_) => at(vec![issue::rejected(
            Step::Output,
            Fault::PublicationUnwind,
        )]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profile_is_the_closed_mapping_of_the_last_segment() {
        for (written, expected) in [
            ("out.csv", Some("csv")),
            ("data.v2/out.PARQUET", Some("parquet")),
            ("out.Csv", Some("csv")),
            ("data.csv/out", None),
            ("out.csv.tmp", None),
            ("out.xpt", None),
            ("out.json", None),
            (".csv", None),
            ("out", None),
            ("", None),
        ] {
            assert_eq!(profile(written), expected, "{written:?}");
        }
        // A Windows path separates its segments with a backslash too; on
        // another host a backslash is part of the file's name.
        let expected = if cfg!(windows) { None } else { Some("csv") };
        assert_eq!(profile("data\\.csv"), expected);
    }

    fn text(value: &str) -> Value {
        Value::String(value.into())
    }

    #[test]
    fn a_parquet_file_needs_the_parquet_codec() {
        let values = [Some("a")].into_iter().collect();
        let column = crate::public_table::PublicColumn::Str(values);
        let table = PublicTable::new(vec![("ID".into(), column)]).unwrap();
        assert_eq!(encode(&table, "csv").unwrap(), b"ID\na\n");
        let parquet = encode(&table, "parquet");
        if cfg!(feature = "parquet") {
            let bytes = parquet.unwrap();
            assert!(bytes.starts_with(b"PAR1") && bytes.ends_with(b"PAR1"));
        } else {
            let [refused] = parquet.unwrap_err().try_into().unwrap();
            assert_eq!(refused.context["step"], text("output"));
            assert_eq!(refused.context["code"], text("parquet_codec_unavailable"));
        }
    }

    #[test]
    fn a_stop_is_recorded_as_a_run_records_it() {
        // A target refused before the save entered the file names no file.
        let [refused] = refusal(Cause::Target(PublicationError::Changed), "out.csv")
            .try_into()
            .unwrap();
        assert_eq!(
            refused.code,
            yamaa_core::diagnostic::ConditionCode::ApplicationRejected
        );
        assert!(refused.spec_paths.is_empty());
        assert_eq!(refused.context.keys().collect::<Vec<_>>(), ["code", "step"]);
        assert_eq!(refused.context["code"], text("target_changed"));
        // A file the save entered names itself and the attempt, at no spec
        // path, whether it failed or a caller's resolver unwound.
        for (cause, code) in [
            (
                Cause::Commit(PublicationError::StagingCollision),
                "staging_collision",
            ),
            (Cause::Unwind(Box::new(())), "publication_unwind"),
        ] {
            let [stopped] = refusal(cause, "out.csv").try_into().unwrap();
            assert!(stopped.spec_paths.is_empty());
            assert_eq!(stopped.context["step"], text("output"));
            assert_eq!(stopped.context["code"], text(code));
            assert_eq!(stopped.context["path"], text("out.csv"));
            assert_eq!(stopped.context["entered"], Value::from(1_usize));
            assert_eq!(stopped.context["completed"], Value::from(0_usize));
        }
    }
}
