//! The closed profile as data: `profile/parquet.toml`, included at build time
//! and read once per process. Each lookup answers from one of its tables, so
//! the reader admits only what the file lists, and the file is the one place
//! each of those rules is written.
use super::format::{Codec, Converted, Encoding, Logical, PageType, Physical};
use arrow_schema::DataType;
use serde::{Deserialize, Deserializer};
use std::sync::OnceLock;
use yamaa_core::value::ColumnType;

const SOURCE: &str = include_str!("../../profile/parquet.toml");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    column: Vec<Column>,
    physical: Vec<PhysicalRow>,
    encoding: Vec<Coded<Encoding>>,
    dictionary: Dictionary,
    levels: Levels,
    page: Vec<Page>,
    codec: Vec<CodecRow>,
}

/// One row of the column types (PARQUET-7).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Column {
    /// The column type such a field reads as.
    #[serde(rename = "type", deserialize_with = "column_type")]
    pub kind: ColumnType,
    physical: Physical,
    #[serde(default)]
    unannotated: bool,
    #[serde(default)]
    logical: Vec<String>,
    #[serde(default)]
    converted: Vec<String>,
    layouts: Vec<String>,
    /// The bits of the signed integer every stored value must fit.
    pub width: Option<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PhysicalRow {
    name: Physical,
    code: i64,
    encodings: Vec<Encoding>,
    statistics: Option<usize>,
    #[serde(default)]
    exact: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Coded<T> {
    name: T,
    code: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Dictionary {
    page: Vec<Encoding>,
    data: Vec<Encoding>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Levels {
    v1: Vec<Encoding>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    name: PageType,
    code: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodecRow {
    name: Codec,
    code: i64,
    #[serde(default = "available")]
    available: bool,
}

fn available() -> bool {
    true
}

fn column_type<'de, D: Deserializer<'de>>(deserializer: D) -> Result<ColumnType, D::Error> {
    use ColumnType::*;
    let name = String::deserialize(deserializer)?;
    [Str, Int, Float, Date, DateTime]
        .into_iter()
        .find(|kind| kind.name() == name)
        .ok_or_else(|| serde::de::Error::custom(format!("no column type {name:?}")))
}

fn profile() -> &'static Profile {
    static PROFILE: OnceLock<Profile> = OnceLock::new();
    PROFILE.get_or_init(|| toml::from_str(SOURCE).expect("the closed Parquet profile reads"))
}

/// The column type row a leaf of `physical` with the annotations `logical`
/// and `converted` reads by, when the file's embedded Arrow schema names it
/// in `layout` (PARQUET-7); `None` when no row admits it.
pub(super) fn column(
    physical: Physical,
    logical: Option<Logical>,
    converted: Converted,
    layout: &DataType,
) -> Option<&'static Column> {
    let logical = logical.map(Logical::spelled);
    let layout = spelled_layout(layout);
    profile().column.iter().find(|row| {
        let annotated = match &logical {
            Some(logical) => row.logical.contains(logical),
            None if converted == Converted::None => row.unannotated,
            None => row.converted.iter().any(|name| name == converted.name()),
        };
        row.physical == physical && annotated && row.layouts.contains(&layout)
    })
}

/// A layout as the profile names it: the Arrow C++ spelling of its type,
/// a dictionary's being its value type's. A zone, even an empty one, makes a
/// zoned timestamp.
fn spelled_layout(layout: &DataType) -> String {
    match layout {
        DataType::Dictionary(_, value) => spelled_layout(value),
        DataType::Timestamp(unit, Some(zone)) => {
            format!("timestamp[{}, tz={zone}]", super::type_name::unit(unit))
        }
        other => {
            let field = arrow_schema::Field::new("", other.clone(), true);
            super::type_name::stored_type(&field, &[])
        }
    }
}

/// The codec of format code `code`, and whether this build decodes it;
/// `None` for a code the profile does not list.
pub(super) fn codec(code: i64) -> Option<(Codec, bool)> {
    let row = profile().codec.iter().find(|row| row.code == code)?;
    Some((row.name, row.available))
}

/// The encoding of format code `code`; `None` for a code the profile does
/// not list.
pub(super) fn encoding(code: i64) -> Option<Encoding> {
    let row = profile().encoding.iter().find(|row| row.code == code)?;
    Some(row.name)
}

/// Whether a dictionary page's values may be in `encoding`.
pub(super) fn dictionary_page(encoding: Encoding) -> bool {
    profile().dictionary.page.contains(&encoding)
}

/// Whether a data page in `encoding` holds dictionary indices.
pub(super) fn dictionary_data(encoding: Encoding) -> bool {
    profile().dictionary.data.contains(&encoding)
}

/// The level encoding of a version 1 data page's format code `code`; `None`
/// for a code the profile does not list for levels.
pub(super) fn levels(code: i64) -> Option<Encoding> {
    encoding(code).filter(|encoding| profile().levels.v1.contains(encoding))
}

/// The physical type of format code `code`; `None` for a code the profile
/// does not list.
pub(super) fn physical(code: i64) -> Option<Physical> {
    let row = profile().physical.iter().find(|row| row.code == code)?;
    Some(row.name)
}

/// The fewest bytes a minimum or maximum statistic of `physical` holds, and
/// whether it holds exactly that many; `None` when the profile bounds none.
pub(super) fn statistics(physical: Physical) -> Option<(usize, bool)> {
    let row = profile().physical.iter().find(|row| row.name == physical)?;
    row.statistics.map(|width| (width, row.exact))
}

/// Whether a data page of `physical` values decodes from `encoding`, a value
/// encoding that holds no dictionary indices.
pub(super) fn decodes(physical: Physical, encoding: Encoding) -> bool {
    profile()
        .physical
        .iter()
        .any(|row| row.name == physical && row.encodings.contains(&encoding))
}

/// The page type of format code `code`; `None` for a code the profile does
/// not list.
pub(super) fn page(code: i64) -> Option<PageType> {
    let row = profile().page.iter().find(|row| row.code == code)?;
    Some(row.name)
}

/// The most encoding statistics a column chunk lists: one for each page
/// type and encoding the profile lists, as a chunk counts the pages of each
/// pair once.
pub(super) fn encoding_statistics() -> usize {
    profile().page.len() * profile().encoding.len()
}

#[cfg(test)]
mod tests {
    //! The data file's own seams: it reads, every table names each member
    //! once, and the lookups answer as its rows say.
    use super::*;

    #[test]
    fn the_profile_reads_and_lists_each_code_and_name_once() {
        let profile = profile();
        fn distinct<T: PartialEq + std::fmt::Debug>(label: &str, items: Vec<T>) {
            for (index, item) in items.iter().enumerate() {
                assert!(
                    !items[..index].contains(item),
                    "{label} lists {item:?} twice"
                );
            }
        }
        distinct(
            "physical names",
            profile.physical.iter().map(|r| r.name).collect(),
        );
        distinct(
            "physical codes",
            profile.physical.iter().map(|r| r.code).collect(),
        );
        distinct(
            "encoding names",
            profile.encoding.iter().map(|r| r.name).collect(),
        );
        distinct(
            "encoding codes",
            profile.encoding.iter().map(|r| r.code).collect(),
        );
        distinct("page names", profile.page.iter().map(|r| r.name).collect());
        distinct("page codes", profile.page.iter().map(|r| r.code).collect());
        distinct(
            "codec names",
            profile.codec.iter().map(|r| r.name).collect(),
        );
        distinct(
            "codec codes",
            profile.codec.iter().map(|r| r.code).collect(),
        );
        // Every encoding a table names is one the format codes.
        let coded: Vec<_> = profile.encoding.iter().map(|row| row.name).collect();
        let named = profile
            .physical
            .iter()
            .flat_map(|row| row.encodings.iter())
            .chain(&profile.dictionary.page)
            .chain(&profile.dictionary.data)
            .chain(&profile.levels.v1);
        for encoding in named {
            assert!(coded.contains(encoding), "{encoding:?} has no code");
        }
    }

    /// One row's column type for a leaf and the layout the embedded schema
    /// names it in, if any.
    fn kind(
        physical: Physical,
        logical: Option<Logical>,
        converted: Converted,
        layout: &DataType,
    ) -> Option<ColumnType> {
        column(physical, logical, converted, layout).map(|row| row.kind)
    }

    /// The text, float, date and datetime rows, as the closed mapping of
    /// PARQUET-7 states them: a leaf reads by its logical type, by its
    /// converted type only without one, and by no annotation only where the
    /// row says so; the layout is the embedded schema's, a dictionary's being
    /// its value type's, and a zone, even an empty one, is not one of them.
    #[test]
    fn a_column_row_admits_its_annotations_and_layouts_alone() {
        use Converted as C;
        use Physical as P;
        let micros = |utc| Logical::Timestamp {
            utc,
            unit: super::super::format::TimeUnit::Micros,
        };
        let dictionary = |value| DataType::Dictionary(Box::new(DataType::Int8), Box::new(value));
        let timestamp = |zone: Option<&str>| {
            DataType::Timestamp(arrow_schema::TimeUnit::Microsecond, zone.map(Into::into))
        };
        let (text, date) = (Some(ColumnType::Str), Some(ColumnType::Date));
        let (float, datetime) = (Some(ColumnType::Float), Some(ColumnType::DateTime));
        let string = Some(Logical::String);
        let cases = [
            (P::ByteArray, string, C::Utf8, DataType::Utf8, text),
            (P::ByteArray, None, C::Utf8, DataType::Utf8View, text),
            (P::ByteArray, None, C::None, DataType::Utf8, None),
            (
                P::ByteArray,
                string,
                C::None,
                dictionary(DataType::LargeUtf8),
                text,
            ),
            (P::ByteArray, string, C::Utf8, DataType::Binary, None),
            (P::Double, None, C::None, DataType::Float64, float),
            (
                P::Double,
                None,
                C::None,
                dictionary(DataType::Float64),
                float,
            ),
            (P::Float, None, C::None, DataType::Float32, None),
            (P::Int32, None, C::Date, DataType::Date32, date),
            (
                P::Int32,
                Some(Logical::Date),
                C::Date,
                DataType::Date32,
                date,
            ),
            (
                P::Int64,
                Some(micros(false)),
                C::None,
                timestamp(None),
                datetime,
            ),
            (
                P::Int64,
                Some(micros(false)),
                C::None,
                timestamp(Some("")),
                None,
            ),
            (
                P::Int64,
                Some(micros(true)),
                C::None,
                timestamp(Some("UTC")),
                None,
            ),
            (P::Int64, None, C::TimestampMicros, timestamp(None), None),
        ];
        for (physical, logical, converted, layout, expected) in cases {
            let label = format!("{physical:?} {logical:?} {converted:?} {layout:?}");
            assert_eq!(
                kind(physical, logical, converted, &layout),
                expected,
                "{label}"
            );
        }
    }

    /// An INT32 or INT64 leaf, unannotated or annotated as a signed integer
    /// whose width fits its physical type, reads as `int` in the layout of its
    /// width, and a narrower annotation bounds every stored value (PARQUET-7).
    /// An unsigned annotation, a width the physical type does not hold, and a
    /// layout of another width or type stay refused.
    #[test]
    fn signed_integer_fields_that_fit_their_physical_type_read_as_int() {
        use Converted as C;
        use Physical as P;
        let int = |bits, signed| Some(Logical::Integer { bits, signed });
        let legacy = |bits: i8, signed| match (bits, signed) {
            (8, true) => C::Int8,
            (16, true) => C::Int16,
            (32, true) => C::Int32,
            (64, true) => C::Int64,
            (8, false) => C::Uint8,
            (16, false) => C::Uint16,
            (32, false) => C::Uint32,
            _ => C::Uint64,
        };
        // An unannotated INT32 or INT64 is a signed integer of its width.
        for (physical, layout) in [(P::Int32, DataType::Int32), (P::Int64, DataType::Int64)] {
            let actual = kind(physical, None, C::None, &layout);
            assert_eq!(actual, Some(ColumnType::Int), "{physical:?}");
        }
        for (physical, bits, layout) in [
            (P::Int64, 64, DataType::Int64),
            (P::Int32, 32, DataType::Int32),
            (P::Int32, 16, DataType::Int16),
            (P::Int32, 8, DataType::Int8),
        ] {
            // The logical type alone, both annotations, and the converted
            // type alone, as DuckDB writes it; then the same unsigned.
            for signed in [true, false] {
                for (logical, converted) in [
                    (int(bits, signed), C::None),
                    (int(bits, signed), legacy(bits, signed)),
                    (None, legacy(bits, signed)),
                ] {
                    let expected = signed.then_some(ColumnType::Int);
                    let actual = kind(physical, logical, converted, &layout);
                    assert_eq!(actual, expected, "{bits}: {logical:?} {converted:?}");
                    let width =
                        column(physical, logical, converted, &layout).and_then(|row| row.width);
                    let bound = (signed && bits < 32).then_some(bits as u8);
                    assert_eq!(width, bound, "{bits}: {logical:?} {converted:?}");
                }
            }
        }
        let micros = DataType::Timestamp(arrow_schema::TimeUnit::Microsecond, None);
        for (physical, logical, converted, layout) in [
            (P::Int32, int(64, true), legacy(64, true), DataType::Int64),
            (P::Int64, int(32, true), legacy(32, true), DataType::Int32),
            (P::Int32, int(8, true), legacy(8, true), DataType::Int16),
            (P::Int64, int(64, true), legacy(64, true), micros),
            (P::Int32, None, legacy(16, true), DataType::Int8),
            (P::Int32, None, C::None, DataType::Int16),
        ] {
            let actual = kind(physical, logical, converted, &layout);
            assert_eq!(
                actual, None,
                "{physical:?} {logical:?} {converted:?} {layout:?}"
            );
        }
    }
}
