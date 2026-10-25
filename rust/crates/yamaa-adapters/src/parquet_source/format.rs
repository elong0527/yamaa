//! The Parquet format's enumerations as the reader names them: the physical
//! types, logical and converted annotations, encodings, codecs and page types
//! of the format's Thrift definition. The closed profile names them in its
//! data file (`closed_profile`), each by its spelling here.
use serde::Deserialize;

/// A leaf's physical type.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Physical {
    Boolean,
    Int32,
    Int64,
    Int96,
    Float,
    Double,
    ByteArray,
    FixedLenByteArray,
}

impl Physical {
    /// The type as the format names it, such as `BYTE_ARRAY`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Boolean => "BOOLEAN",
            Self::Int32 => "INT32",
            Self::Int64 => "INT64",
            Self::Int96 => "INT96",
            Self::Float => "FLOAT",
            Self::Double => "DOUBLE",
            Self::ByteArray => "BYTE_ARRAY",
            Self::FixedLenByteArray => "FIXED_LEN_BYTE_ARRAY",
        }
    }
}

/// A value encoding, or a level encoding.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Encoding {
    Plain,
    PlainDictionary,
    Rle,
    BitPacked,
    DeltaBinaryPacked,
    DeltaLengthByteArray,
    DeltaByteArray,
    RleDictionary,
    ByteStreamSplit,
}

/// A page's compression codec.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Codec {
    Uncompressed,
    Snappy,
    Gzip,
    Lzo,
    Brotli,
    Lz4,
    Zstd,
    Lz4Raw,
}

/// A page's type.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum PageType {
    DataPage,
    IndexPage,
    DictionaryPage,
    DataPageV2,
}

/// A time or timestamp's unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TimeUnit {
    Millis,
    Micros,
    Nanos,
}

impl TimeUnit {
    fn name(self) -> &'static str {
        match self {
            Self::Millis => "MILLIS",
            Self::Micros => "MICROS",
            Self::Nanos => "NANOS",
        }
    }
}

/// A logical type annotation that the format's Thrift definition declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Logical {
    String,
    Map,
    List,
    Enum,
    Decimal { scale: i32, precision: i32 },
    Date,
    Time { utc: bool, unit: TimeUnit },
    Timestamp { utc: bool, unit: TimeUnit },
    Integer { bits: i8, signed: bool },
    Unknown,
    Json,
    Bson,
    Uuid,
    Float16,
}

impl Logical {
    /// The annotation as the closed profile spells it, such as
    /// `Int(16, signed)` or `Timestamp(MICROS, isAdjustedToUTC=false)`.
    pub(crate) fn spelled(self) -> String {
        match self {
            Self::String => "String".into(),
            Self::Map => "Map".into(),
            Self::List => "List".into(),
            Self::Enum => "Enum".into(),
            Self::Decimal { scale, precision } => format!("Decimal({precision}, {scale})"),
            Self::Date => "Date".into(),
            Self::Time { utc, unit } => {
                format!("Time({}, isAdjustedToUTC={utc})", unit.name())
            }
            Self::Timestamp { utc, unit } => {
                format!("Timestamp({}, isAdjustedToUTC={utc})", unit.name())
            }
            Self::Integer { bits, signed } => {
                format!(
                    "Int({bits}, {})",
                    if signed { "signed" } else { "unsigned" }
                )
            }
            Self::Unknown => "Unknown".into(),
            Self::Json => "Json".into(),
            Self::Bson => "Bson".into(),
            Self::Uuid => "Uuid".into(),
            Self::Float16 => "Float16".into(),
        }
    }

    /// The annotation as a column's signature spells it, as parquet 55
    /// named its logical types, such as `Integer { bit_width: 8,
    /// is_signed: true }` (`parquet_source::ColumnSignature`).
    pub(crate) fn debug(self) -> String {
        let unit = |unit: TimeUnit| match unit {
            TimeUnit::Millis => "MILLIS(MilliSeconds)",
            TimeUnit::Micros => "MICROS(MicroSeconds)",
            TimeUnit::Nanos => "NANOS(NanoSeconds)",
        };
        match self {
            Self::Decimal { scale, precision } => {
                format!("Decimal {{ scale: {scale}, precision: {precision} }}")
            }
            Self::Time { utc, unit: time } => format!(
                "Time {{ is_adjusted_to_u_t_c: {utc}, unit: {} }}",
                unit(time)
            ),
            Self::Timestamp { utc, unit: time } => format!(
                "Timestamp {{ is_adjusted_to_u_t_c: {utc}, unit: {} }}",
                unit(time)
            ),
            Self::Integer { bits, signed } => {
                format!("Integer {{ bit_width: {bits}, is_signed: {signed} }}")
            }
            other => format!("{other:?}"),
        }
    }
}

/// A legacy converted type annotation, or none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Converted {
    None,
    Utf8,
    Map,
    MapKeyValue,
    List,
    Enum,
    Decimal,
    Date,
    TimeMillis,
    TimeMicros,
    TimestampMillis,
    TimestampMicros,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Int8,
    Int16,
    Int32,
    Int64,
    Json,
    Bson,
    Interval,
}

impl Converted {
    /// The annotation as the format names it, such as `UTF8` or `INT_16`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Utf8 => "UTF8",
            Self::Map => "MAP",
            Self::MapKeyValue => "MAP_KEY_VALUE",
            Self::List => "LIST",
            Self::Enum => "ENUM",
            Self::Decimal => "DECIMAL",
            Self::Date => "DATE",
            Self::TimeMillis => "TIME_MILLIS",
            Self::TimeMicros => "TIME_MICROS",
            Self::TimestampMillis => "TIMESTAMP_MILLIS",
            Self::TimestampMicros => "TIMESTAMP_MICROS",
            Self::Uint8 => "UINT_8",
            Self::Uint16 => "UINT_16",
            Self::Uint32 => "UINT_32",
            Self::Uint64 => "UINT_64",
            Self::Int8 => "INT_8",
            Self::Int16 => "INT_16",
            Self::Int32 => "INT_32",
            Self::Int64 => "INT_64",
            Self::Json => "JSON",
            Self::Bson => "BSON",
            Self::Interval => "INTERVAL",
        }
    }
}
