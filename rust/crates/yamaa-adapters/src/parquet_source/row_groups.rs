//! parquet 55.2.0's decoding of the admitted footer, which the reader read
//! by until #3878: the differential harness's oracle for the native footer
//! (`footer`). It is shown the footer's presentation (`wire::Presentation`).
use super::reader::Error;
use super::{compact, wire};
use parquet::file::metadata::{ParquetMetaData, ParquetMetaDataReader};

/// Decode the admitted footer `held` through its native presentation, with
/// the edits that admission found it needs (`wire::Presentation`).
pub(super) fn decode(held: &[u8], shown: &wire::Presentation) -> Result<ParquetMetaData, Error> {
    let native = shown.apply(compact::native_metadata(held)?);
    super::physical::native(|| ParquetMetaDataReader::decode_metadata(&native))
}
