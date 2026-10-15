//! Shared physical source admission for storage adapters.
pub use yamaa_core::csv_source::{
    parse, parse_with_admission, CsvSource, Error, Field, Limits, ProfileCause,
};
