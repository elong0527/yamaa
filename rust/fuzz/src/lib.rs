//! Shared harness for the fuzz targets: source declarations, the shared source
//! decode both host routes call, and the panic policy.
//!
//! Every target feeds its input to an entry point the hosts use, and that
//! entry point applies the fixed `Limits` itself, so no target sets a bound of
//! its own. A crash, a sanitizer report, a panic that escapes the entry point
//! or a broken target invariant aborts the input, which fails the run.

use std::{
    backtrace::{Backtrace, BacktraceStatus},
    cell::RefCell,
    convert::Infallible,
    panic::{self, AssertUnwindSafe},
};

use yamaa_adapters::{
    arrow_table::ArrowTable, project_source_decoder::Decoder, specification_run::Error,
};
use yamaa_core::{
    specification::{SourceDeclaration, SourceInterpretation, SourceProfile},
    specification_run::SourceDecoder as _,
    table::TableAccess,
    value::ColumnType,
};

thread_local! {
    static PANICS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// libFuzzer's hook aborts on every panic, even one the engine contains by
/// design: the Parquet reader turns a panic in the third-party decoder into a
/// malformed source, and each ReadStat callback turns one into an internal
/// error before it reaches C. This hook only records the message, and [`run`]
/// aborts when a panic escapes the entry point.
pub fn contain_panics() {
    panic::set_hook(Box::new(|info| {
        let backtrace = Backtrace::capture();
        let message = match backtrace.status() {
            BacktraceStatus::Captured => format!("{info}\n{backtrace}"),
            _ => info.to_string(),
        };
        PANICS.with(|panics| panics.borrow_mut().push(message));
    }));
}

/// Run one input. A panic that escapes `body` aborts the process after
/// printing every panic the input raised, contained ones first, so a target
/// invariant that fails on an internal error also shows the contained cause.
pub fn run(body: impl FnOnce()) {
    PANICS.with(|panics| panics.borrow_mut().clear());
    if panic::catch_unwind(AssertUnwindSafe(body)).is_err() {
        PANICS.with(|panics| {
            for message in panics.borrow().iter() {
                eprintln!("{message}");
            }
        });
        std::process::abort();
    }
}

/// One input declaration, as a specification's `input` entry would declare it.
/// Empty strings stay missing, the default convention.
pub fn source(
    profile: SourceProfile,
    types: Vec<(String, ColumnType)>,
    encoding: Option<&str>,
) -> SourceDeclaration {
    let path = match profile {
        SourceProfile::Csv => "raw.csv",
        SourceProfile::Parquet => "raw.parquet",
        SourceProfile::Xport => "raw.xpt",
        SourceProfile::Sas7bdat => "raw.sas7bdat",
    };
    SourceDeclaration {
        name: "RAW".into(),
        path: path.into(),
        types,
        profile,
        encoding: encoding.map(Into::into),
        empty_string_present: false,
        ordinal: None,
    }
}

/// Decode held bytes through the source decode that both host routes call:
/// the profile selects the CSV, Parquet or ReadStat reader, and the declared
/// types and interpretation apply as they do for a specification input.
pub fn decode(
    source: &SourceDeclaration,
    bytes: &[u8],
    interpretation: SourceInterpretation,
) -> Result<ArrowTable, Error> {
    Decoder::<Infallible>::default()
        .decode(source, bytes, interpretation, None)
        .map(|snapshot| snapshot.into_table())
}

/// Read one SAS file under the shared decode: undeclared in windows-1252, the
/// fixed ODM fields in UTF-8, then every numeric field declared as each
/// narrower type in turn. Input bytes must be read or refused as a source
/// finding, never as an internal error: the reader returns one only when a
/// callback contained a panic or ReadStat reported an allocation failure, and
/// under libFuzzer's allocation limit that failure can only be an empty
/// allocation ReadStat refuses.
pub fn sas(profile: SourceProfile, bytes: &[u8]) {
    let undeclared = decode(
        &source(profile, Vec::new(), Some("windows-1252")),
        bytes,
        SourceInterpretation::Ordinary,
    );
    refuse_internal(&undeclared);
    refuse_internal(&decode(
        &source(profile, Vec::new(), Some("utf-8")),
        bytes,
        SourceInterpretation::Odm,
    ));
    let Ok(table) = undeclared else {
        return;
    };
    let numeric: Vec<String> = table
        .schema()
        .columns()
        .iter()
        .filter(|column| column.kind == ColumnType::Float)
        .map(|column| column.name.clone())
        .collect();
    if numeric.is_empty() {
        return;
    }
    for kind in [ColumnType::Int, ColumnType::Date, ColumnType::DateTime] {
        let types = numeric.iter().map(|name| (name.clone(), kind)).collect();
        refuse_internal(&decode(
            &source(profile, types, Some("windows-1252")),
            bytes,
            SourceInterpretation::Ordinary,
        ));
    }
}

fn refuse_internal(result: &Result<ArrowTable, Error>) {
    if let Err(Error::SasSource(yamaa_adapters::sas_source::Error::Internal)) = result {
        panic!("the SAS reader returned an internal error for input bytes");
    }
}
