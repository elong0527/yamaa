//! The host-internal seam (#5080): what the Python and R bindings and the
//! command line's binary share with the facade beyond its frozen surface,
//! each of them one judgement the clients would otherwise repeat.
//!
//! The module is `#[doc(hidden)]`. `public-api.txt` leaves it out, its
//! generator refuses a frozen item that names one of its types, and the 1.0
//! stability contract does not cover it (`rust/interface/README.md`): any
//! release may change it. An embedder uses the frozen surface instead.

use std::ffi::OsString;
use std::num::NonZeroUsize;

use crate::diagnostic::context_value_to_core;
use crate::host::functions::HostPayload;
use crate::text::Action;
use crate::{
    ContextValue, Failure, HostLanguage, OdmOptions, Partitions, Run, Summary, TableError,
};

mod locks;

pub use locks::{
    decode_renv_lock, verify_uv_lock, InstalledDistributions, InstalledVersion, RenvLock,
};

/// What `run` observed, as JSON: what it read and built, the project
/// functions it called and the files its saves wrote, missing when the run
/// never began; with `measured`, beside the measurements its operation took
/// (the time of each stage, the calls and save attempts it counted, and how
/// it got its plan), which a run that never began still has. This is
/// installed-run evidence, which the hosts' qualification and plan-reuse
/// suites read through the engine; its members are not part of the
/// interface.
pub fn observations(run: &Run, measured: bool) -> Option<String> {
    match measured {
        true => Some(run.build.measured_observations()),
        false => run.build.observations(),
    }
}

/// The partitions with the threads a host's `threads` argument gives
/// (#4193): a positive whole number, which saturates at the most this
/// platform counts, or a missing value, which keeps the default
/// ([`Partitions::with_threads`]). Only R gives a whole number as a double,
/// as its numeric literals are; Python's `threads` is an `int`
/// (`bindings.yaml`, `types.threads`). `None` for any other value, which
/// the host refuses as an argument it cannot take.
pub fn threads(
    language: HostLanguage,
    partitions: Partitions,
    threads: &ContextValue,
) -> Option<Partitions> {
    if matches!(threads, ContextValue::Null) {
        return Some(partitions);
    }
    let count = match whole(language, threads)? {
        Whole::Count(count) => usize::try_from(count).unwrap_or(usize::MAX),
        Whole::Beyond => usize::MAX,
        Whole::Negative => return None,
    };
    NonZeroUsize::new(count).map(|count| partitions.with_threads(count))
}

/// The ODM options a host's limit arguments give, admitted before `source`
/// is read. A whole number within each limit's range is admitted, which
/// only R may give as a double; a Boolean, missing, fractional, negative or
/// unrepresentable value, and a Python `float`, refuse as
/// `value_not_permitted` at the argument, with the value in its diagnostic
/// context. Validation follows the reader's order: bytes, members, then
/// depth.
pub fn odm_options(
    language: HostLanguage,
    source: &str,
    max_archive_members: ContextValue,
    max_expanded_bytes: ContextValue,
    max_xml_depth: ContextValue,
) -> Result<OdmOptions, Failure> {
    use yamaa_core::odm_items::Options;
    let admit = |name: &str, minimum: u64, maximum: u64, value: ContextValue| {
        let number = match whole(language, &value) {
            Some(Whole::Count(number)) => Some(number),
            _ => None,
        };
        if let Some(number) = number.filter(|number| (minimum..=maximum).contains(number)) {
            return Ok(number);
        }
        let shown = context_value_to_core(&integral(value));
        let issue = yamaa_core::diagnostic::invalid_odm_limit(name, minimum, shown);
        Err(Failure::from_issues(Action::Read(source), &[issue]))
    };
    let largest = usize::MAX as u64;
    let bytes = admit(
        "max_expanded_bytes",
        Options::MIN_EXPANDED_BYTES,
        u64::MAX,
        max_expanded_bytes,
    )?;
    let members = admit(
        "max_archive_members",
        Options::MIN_ARCHIVE_MEMBERS,
        largest,
        max_archive_members,
    )?;
    let depth = admit(
        "max_xml_depth",
        Options::MIN_XML_DEPTH as u64,
        largest,
        max_xml_depth,
    )?;
    Ok(OdmOptions::new()
        .with_max_expanded_bytes(bytes)
        .with_max_archive_members(members as usize)
        .with_max_xml_depth(depth as usize))
}

/// The refusal of a run of `specification` whose host frame for `alias`
/// could not be imported: a validation `value_not_permitted` at
/// `inputs.<alias>`, retaining the stable table code and naming the refused
/// column by its name and Arrow type (and record), or else the import
/// message, with no files written. Both hosts refuse an input frame so.
pub fn input_failure(error: TableError, specification: &str, alias: &str) -> Failure {
    error.at_input(specification, alias)
}

/// `batches` as a standard Arrow C stream, as [`crate::Batches::into_stream`]
/// gives it, which keeps `lifetime` until the foreign consumer releases its
/// reader: the hosts keep their callbacks and the original causes so.
#[cfg(feature = "arrow")]
pub fn stream_with_lifetime(
    batches: crate::Batches,
    lifetime: impl Send + 'static,
) -> crate::FFI_ArrowArrayStream {
    batches.into_stream_with_lifetime(lifetime)
}

/// The original progress callback exception of `failure`, when the host
/// supplied one, which the bindings chain onto their refusal without
/// rendering it again.
pub fn failure_cause(failure: &Failure) -> Option<&HostPayload> {
    failure.cause.as_ref()
}

/// The printed text of a lazy specification object, which has read
/// nothing: `yamaa spec <path>`, with the path as given.
pub fn spec_summary(path: &str) -> Summary {
    Summary::lazy("spec", path)
}

/// The printed text of a lazy project object: `yamaa project <path>`.
pub fn project_summary(path: &str) -> Summary {
    Summary::lazy("project", path)
}

/// Whether `arguments` name a command with cancellation checkpoints,
/// `check`, `run` or `submission`, whose token the client polls; the other
/// commands run to their end.
pub fn cooperative(arguments: &[OsString]) -> bool {
    crate::cli::cooperative(arguments)
}

/// The largest magnitude up to which every whole number is a double: a
/// double beyond it may already have been rounded.
const EXACT_DOUBLE: f64 = 9_007_199_254_740_992.0;

/// A host number read as a whole number.
enum Whole {
    /// Zero or more, within 64 bits.
    Count(u64),
    /// More than 64 bits count.
    Beyond,
    /// Below zero.
    Negative,
}

/// `value` as a whole number: an integer, of any width, or, from R alone,
/// a double that holds one exactly. `None` for any other value.
fn whole(language: HostLanguage, value: &ContextValue) -> Option<Whole> {
    match value {
        ContextValue::Int(value) => {
            Some(u64::try_from(*value).map_or(Whole::Negative, Whole::Count))
        }
        // An integer beyond 64 bits, as Python gives it.
        ContextValue::Wide(text) => {
            let digits = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
            match text.strip_prefix('-') {
                Some(rest) if digits(rest) => Some(Whole::Negative),
                None if digits(text) => {
                    Some(text.parse::<u64>().map_or(Whole::Beyond, Whole::Count))
                }
                _ => None,
            }
        }
        ContextValue::Float(value)
            if language == HostLanguage::R
                && value.is_finite()
                && value.fract() == 0.0
                && value.abs() <= EXACT_DOUBLE =>
        {
            Some(match *value < 0.0 {
                true => Whole::Negative,
                false => Whole::Count(*value as u64),
            })
        }
        _ => None,
    }
}

/// `value` as a diagnostic shows it: an exact whole double, which only R
/// admits, has the spelling of the integer Python gives.
fn integral(value: ContextValue) -> ContextValue {
    match value {
        ContextValue::Float(number)
            if number.is_finite() && number.fract() == 0.0 && number.abs() <= EXACT_DOUBLE =>
        {
            ContextValue::Int(number as i64)
        }
        value => value,
    }
}
