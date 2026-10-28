//! The differential harness: the native reader against parquet 55.2.0, while
//! both exist (#3878). Every input reads alike through the reader's native
//! decoders (`reader::NATIVE`) and through parquet 55's (`UPSTREAM`), which
//! the reader read with until then, the reader around them the same: the
//! footer the two decode, the columns' signatures, the table schema each
//! read gives from the footer alone, and the table, or the first finding,
//! of an ordinary read and of an ODM read.
//!
//! The inputs are the benchmark corpus, which the `parquet` fuzz target's
//! seeds are (it seeds every committed `.parquet` file, and binary files live
//! only under `benchmarks/`), the fuzz target's regression inputs, the
//! host fixtures' Parquet files, and the inputs the integration tests write
//! (`test_files`). Each footer is also mutated one field at a time: a field
//! dropped, an integer replaced by each of a set of values that covers every
//! code the format's enumerations use and some it does not, a binary emptied
//! or made longer or not UTF-8, a boolean flipped, a struct emptied and a
//! list element dropped. So is each page header, where the change keeps its
//! length, and each page's bytes, one byte at a time.
use super::columns::Column;
use super::footer::{self, Chunk, Footer, Kind, Leaf, Node, Repetition, RowGroup, Schema};
use super::format::{Codec, Converted, Encoding, Logical, Physical, TimeUnit};
use super::reader::{self, Decoders, Error, NATIVE};
use super::{compact, framing, metadata, physical, row_groups, ColumnSignature};
use arrow_array::RecordBatch;
use parquet::file::metadata::ParquetMetaData;
use std::path::{Path, PathBuf};
use yamaa_core::table::TableSchema;

/// The repository's root.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Every `.parquet` file below `directory`, in path order.
fn parquet_files(directory: &Path, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            parquet_files(&path, found);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "parquet")
        {
            found.push(path);
        }
    }
}

fn unhex(text: &str) -> Vec<u8> {
    let digits: String = text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .flat_map(|line| line.chars().filter(|c| !c.is_whitespace()))
        .collect();
    (0..digits.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&digits[at..at + 2], 16).unwrap())
        .collect()
}

/// Every hex string under a `hex` key of a host-fixture family.
fn host_files(value: &yamaa_core::json::Value, label: &str, found: &mut Vec<(String, Vec<u8>)>) {
    use yamaa_core::json::Value;
    match value {
        Value::Object(members) => {
            for (key, member) in members {
                match (key.as_str(), member) {
                    ("hex", Value::String(hex)) => found.push((label.into(), unhex(hex))),
                    _ => host_files(member, &format!("{label}/{key}"), found),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                host_files(item, label, found);
            }
        }
        _ => {}
    }
}

/// The names of the inputs the integration tests write (`test_files`).
fn written() -> Vec<String> {
    let mut names = Vec::new();
    for codec in ["none", "snappy", "gzip", "brotli", "lz4", "zstd"] {
        for version in ["1", "2"] {
            for dictionary in ["0", "1"] {
                names.push(format!("{codec}-{version}-{dictionary}"));
            }
        }
    }
    let others = [
        "delta",
        "empty",
        "nonfinite",
        "text",
        "boolean",
        "fractional",
        "timestamp-units",
        "fractional-milliseconds",
        "fractional-nanoseconds",
        "beyond-milliseconds",
        "float32-values",
        "bad-date",
        "empty-name",
        "duplicate",
        "date64",
        "mixed-struct",
        "mixed-map",
        "mixed-empty",
        "mixed-duplicate",
        "utf8",
        "utf8-bool",
        "utf8-time",
        "odm-vendor",
        "odm-vendor-malformed",
        "odm-missing",
        "odm-wrong-type",
        "odm-ambiguous",
        "odm-empty",
    ];
    names.extend(others.map(String::from));
    let unsupported = [
        "int32",
        "uint64",
        "float32",
        "binary",
        "milliseconds",
        "timezone",
        "list",
        "fixed-list",
        "struct",
        "decimal",
        "decimal32",
        "decimal64",
        "list-view",
        "large-list-view",
        "geometry",
        "geography",
        "variant",
    ];
    names.extend(unsupported.map(|name| format!("unsupported-{name}")));
    names
}

/// The harness's inputs, each with a label.
fn inputs() -> Vec<(String, Vec<u8>)> {
    let root = root();
    let mut corpus = Vec::new();
    parquet_files(&root.join("benchmarks"), &mut corpus);
    assert!(
        !corpus.is_empty(),
        "the benchmark corpus holds Parquet files"
    );
    let mut inputs: Vec<_> = corpus
        .iter()
        .map(|path| {
            let label = path.strip_prefix(&root).unwrap().display().to_string();
            (label, std::fs::read(path).unwrap())
        })
        .collect();
    let regressions = root.join("rust/fuzz/regressions/parquet");
    if regressions.is_dir() {
        let mut paths: Vec<_> = std::fs::read_dir(&regressions)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "hex"))
            .collect();
        paths.sort();
        for path in paths {
            let label = path.strip_prefix(&root).unwrap().display().to_string();
            inputs.push((label, unhex(&std::fs::read_to_string(&path).unwrap())));
        }
    }
    let truth =
        crate::json_wire::decode(include_str!("../../../../tests/host-fixtures/cases.json"))
            .unwrap();
    let mut hosts = Vec::new();
    host_files(&truth, "host-fixtures", &mut hosts);
    hosts.retain(|(_, bytes)| bytes.starts_with(b"PAR1"));
    assert!(!hosts.is_empty(), "the host fixtures hold Parquet files");
    inputs.extend(hosts);
    for name in written() {
        inputs.push((format!("written {name}"), super::test_files::file(&name)));
    }
    inputs
}

/// A compact Thrift value, held so that one field can be changed and the
/// whole written again.
#[derive(Clone, Debug)]
enum Thrift {
    Bool(bool),
    /// An integer of wire type 3 to 6.
    Integer(u8, i64),
    Double([u8; 8]),
    Binary(Vec<u8>),
    /// A list (9) or set (10) of elements of one wire type.
    Sequence(u8, u8, Vec<Thrift>),
    Map(u8, u8, Vec<(Thrift, Thrift)>),
    Struct(Vec<(i16, Thrift)>),
    Uuid([u8; 16]),
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> u8 {
        self.at += 1;
        self.bytes[self.at - 1]
    }
    fn varint(&mut self) -> u64 {
        let (mut value, mut shift) = (0u64, 0);
        loop {
            let byte = self.byte();
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return value;
            }
            shift += 7;
        }
    }
    fn zigzag(&mut self) -> i64 {
        let value = self.varint();
        (value >> 1) as i64 ^ -((value & 1) as i64)
    }
    fn take(&mut self, count: usize) -> Vec<u8> {
        self.at += count;
        self.bytes[self.at - count..self.at].to_vec()
    }
    fn value(&mut self, kind: u8) -> Thrift {
        match kind {
            1 => Thrift::Bool(true),
            2 => Thrift::Bool(false),
            3 => Thrift::Integer(3, i64::from(self.byte() as i8)),
            4..=6 => Thrift::Integer(kind, self.zigzag()),
            7 => Thrift::Double(self.take(8).try_into().unwrap()),
            8 => {
                let length = self.varint() as usize;
                Thrift::Binary(self.take(length))
            }
            9 | 10 => {
                let header = self.byte();
                let count = match header >> 4 {
                    15 => self.varint() as usize,
                    count => usize::from(count),
                };
                let element = header & 15;
                let items = (0..count).map(|_| self.element(element)).collect();
                Thrift::Sequence(kind, element, items)
            }
            11 => {
                let count = self.varint() as usize;
                let kinds = if count == 0 { 0 } else { self.byte() };
                let pairs = (0..count)
                    .map(|_| (self.element(kinds >> 4), self.element(kinds & 15)))
                    .collect();
                Thrift::Map(kinds >> 4, kinds & 15, pairs)
            }
            12 => {
                let mut fields = Vec::new();
                let mut previous = 0i16;
                loop {
                    let header = self.byte();
                    if header & 15 == 0 {
                        return Thrift::Struct(fields);
                    }
                    let id = match header >> 4 {
                        0 => self.zigzag() as i16,
                        delta => previous + i16::from(delta),
                    };
                    previous = id;
                    fields.push((id, self.value(header & 15)));
                }
            }
            _ => Thrift::Uuid(self.take(16).try_into().unwrap()),
        }
    }
    fn element(&mut self, kind: u8) -> Thrift {
        match kind {
            1 | 2 => Thrift::Bool(self.byte() == 1),
            kind => self.value(kind),
        }
    }
}

fn write_varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 0x80 {
        output.push(value as u8 | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

fn write_zigzag(value: i64, output: &mut Vec<u8>) {
    write_varint(((value << 1) ^ (value >> 63)) as u64, output);
}

impl Thrift {
    /// The wire type a field of this value carries.
    fn kind(&self) -> u8 {
        match self {
            Self::Bool(true) => 1,
            Self::Bool(false) => 2,
            Self::Integer(kind, _) | Self::Sequence(kind, ..) => *kind,
            Self::Double(_) => 7,
            Self::Binary(_) => 8,
            Self::Map(..) => 11,
            Self::Struct(_) => 12,
            Self::Uuid(_) => 13,
        }
    }
    fn write(&self, output: &mut Vec<u8>) {
        match self {
            Self::Bool(_) => {}
            Self::Integer(3, value) => output.push(*value as u8),
            Self::Integer(_, value) => write_zigzag(*value, output),
            Self::Double(bytes) => output.extend(bytes),
            Self::Binary(bytes) => {
                write_varint(bytes.len() as u64, output);
                output.extend(bytes);
            }
            Self::Sequence(_, element, items) => {
                if items.len() < 15 {
                    output.push((items.len() as u8) << 4 | element);
                } else {
                    output.push(0xf0 | element);
                    write_varint(items.len() as u64, output);
                }
                for item in items {
                    item.write_element(output);
                }
            }
            Self::Map(key, value, pairs) => {
                write_varint(pairs.len() as u64, output);
                if !pairs.is_empty() {
                    output.push(key << 4 | value);
                }
                for (key, value) in pairs {
                    key.write_element(output);
                    value.write_element(output);
                }
            }
            Self::Struct(fields) => {
                let mut previous = 0i16;
                for (id, value) in fields {
                    match id - previous {
                        delta @ 1..=15 => output.push((delta as u8) << 4 | value.kind()),
                        _ => {
                            output.push(value.kind());
                            write_zigzag(i64::from(*id), output);
                        }
                    }
                    previous = *id;
                    value.write(output);
                }
                output.push(0);
            }
            Self::Uuid(bytes) => output.extend(bytes),
        }
    }
    fn write_element(&self, output: &mut Vec<u8>) {
        match self {
            Self::Bool(value) => output.push(if *value { 1 } else { 2 }),
            value => value.write(output),
        }
    }

    /// Note each mutation of this value, at `path`, or of a value inside it.
    /// Values are taken from `INTEGERS` and `BINARIES` in turn, `turn`
    /// counting them, so that the values of a footer take each between them.
    fn sites(&self, path: &mut Vec<usize>, turn: &mut usize, found: &mut Vec<Site>) {
        // Every code of the format's enumerations, the widths of an integer
        // annotation, and codes and widths beside them.
        const INTEGERS: [i64; 25] = [
            -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
            64,
        ];
        const BINARIES: [&[u8]; 5] = [b"", b"A", &[7; 12], &[7; 13], &[0xff]];
        let mut replace = |value: Thrift| found.push(Site::Replace(path.clone(), value));
        match self {
            Self::Bool(value) => replace(Self::Bool(!value)),
            // A small integer, a code or a width or a count, takes four of
            // `INTEGERS`; a larger one, an offset or a size, becomes -1.
            Self::Integer(kind, value) if (0..=64).contains(value) => {
                for step in 0..4 {
                    let other = INTEGERS[(*turn * 4 + step) % INTEGERS.len()];
                    if other != *value && (*kind != 3 || other < 128) {
                        replace(Self::Integer(*kind, other));
                    }
                }
                *turn += 1;
            }
            Self::Integer(kind, _) => replace(Self::Integer(*kind, -1)),
            Self::Binary(bytes) => {
                let other = BINARIES[*turn % BINARIES.len()];
                if other != bytes.as_slice() {
                    replace(Self::Binary(other.to_vec()));
                }
                *turn += 1;
            }
            Self::Sequence(_, _, items) => {
                for (index, item) in items.iter().enumerate() {
                    path.push(index);
                    found.push(Site::Drop(path.clone()));
                    item.sites(path, turn, found);
                    path.pop();
                }
            }
            Self::Struct(fields) => {
                replace(Self::Struct(Vec::new()));
                for (index, (_, value)) in fields.iter().enumerate() {
                    path.push(index);
                    found.push(Site::Drop(path.clone()));
                    value.sites(path, turn, found);
                    path.pop();
                }
            }
            Self::Double(_) | Self::Map(..) | Self::Uuid(_) => {}
        }
    }

    /// The value at `path` below this one, and its container's children.
    fn at(&mut self, path: &[usize]) -> &mut Self {
        let Some((&first, rest)) = path.split_first() else {
            return self;
        };
        match self {
            Self::Sequence(_, _, items) => items[first].at(rest),
            Self::Struct(fields) => fields[first].1.at(rest),
            _ => unreachable!("a path leads through containers"),
        }
    }

    /// This value with the mutation `site` made.
    fn mutated(&self, site: &Site) -> Self {
        let mut tree = self.clone();
        match site {
            Site::Replace(path, value) => *tree.at(path) = value.clone(),
            Site::Drop(path) => {
                let (&last, parent) = path.split_last().unwrap();
                match tree.at(parent) {
                    Self::Sequence(_, _, items) => drop(items.remove(last)),
                    Self::Struct(fields) => drop(fields.remove(last)),
                    _ => unreachable!("a value is dropped from a container"),
                }
            }
        }
        tree
    }
}

/// One mutation of a footer: a value replaced, or a field or element
/// dropped, at a path of positions among each container's children.
enum Site {
    Replace(Vec<usize>, Thrift),
    Drop(Vec<usize>),
}

/// The file `bytes` with its footer replaced by `footer`.
fn with_footer(bytes: &[u8], footer: &[u8]) -> Vec<u8> {
    let length = u32::from_le_bytes(bytes[bytes.len() - 8..bytes.len() - 4].try_into().unwrap());
    let start = bytes.len() - 8 - length as usize;
    let mut file = bytes[..start].to_vec();
    file.extend(footer);
    file.extend((footer.len() as u32).to_le_bytes());
    file.extend(b"PAR1");
    file
}

/// The most mutations of one footer the harness makes, evenly spread over
/// all of them.
const MUTATIONS: usize = 250;

/// One-field mutations of the footer of an admitted file, the integers'
/// turn starting at `turn`.
fn mutated(bytes: &[u8], mut turn: usize) -> Vec<Vec<u8>> {
    let Ok(frame) = framing::admit(bytes) else {
        return Vec::new();
    };
    let tree = Reader {
        bytes: frame.metadata,
        at: 0,
    }
    .value(12);
    let mut sites = Vec::new();
    tree.sites(&mut Vec::new(), &mut turn, &mut sites);
    let stride = sites.len().div_ceil(MUTATIONS).max(1);
    sites
        .iter()
        .step_by(stride)
        .map(|site| {
            let mut footer = Vec::new();
            tree.mutated(site).write(&mut footer);
            with_footer(bytes, &footer)
        })
        .collect()
}

/// The footer parquet 55 decoded, in the native model's terms.
fn upstream(metadata: &ParquetMetaData) -> Footer {
    let file = metadata.file_metadata();
    let descriptor = file.schema_descr();
    let mut schema = Schema {
        nodes: Vec::new(),
        leaves: Vec::new(),
    };
    node(descriptor.root_schema(), &mut schema);
    for (index, column) in descriptor.columns().iter().enumerate() {
        let leaf = &mut schema.leaves[index];
        leaf.root = descriptor.get_column_root_idx(index);
        leaf.path = column.path().parts().to_vec();
        leaf.max_definition = column.max_def_level();
        leaf.max_repetition = column.max_rep_level();
    }
    let row_groups = metadata
        .row_groups()
        .iter()
        .map(|group| RowGroup {
            num_rows: group.num_rows(),
            columns: group
                .columns()
                .iter()
                .map(|chunk| Chunk {
                    encodings: chunk
                        .encodings()
                        .iter()
                        .map(|e| encoding(&format!("{e:?}")))
                        .collect(),
                    codec: codec(&format!("{:?}", chunk.compression())),
                    num_values: chunk.num_values(),
                    data_page_offset: chunk.data_page_offset(),
                    dictionary_page_offset: chunk.dictionary_page_offset(),
                    total_compressed_size: chunk.compressed_size(),
                })
                .collect(),
        })
        .collect();
    Footer {
        num_rows: file.num_rows(),
        key_values: file
            .key_value_metadata()
            .into_iter()
            .flatten()
            .map(|item| (item.key.clone(), item.value.clone()))
            .collect(),
        schema,
        row_groups,
    }
}

/// Add `stored` and the nodes below it to `schema`, as the native model
/// orders them; return its position.
fn node(stored: &parquet::schema::types::Type, schema: &mut Schema) -> usize {
    use parquet::basic::Repetition as R;
    use parquet::schema::types::Type;
    let info = stored.get_basic_info();
    let position = schema.nodes.len();
    let repetition = info.has_repetition().then(|| match info.repetition() {
        R::REQUIRED => Repetition::Required,
        R::OPTIONAL => Repetition::Optional,
        R::REPEATED => Repetition::Repeated,
    });
    let converted = converted_type(info.converted_type());
    let logical = info.logical_type().as_ref().map(logical_type);
    schema.nodes.push(Node {
        name: info.name().into(),
        repetition,
        converted,
        logical,
        kind: Kind::Group(Vec::new()),
    });
    match stored {
        Type::PrimitiveType {
            physical_type: stored_physical,
            type_length,
            scale,
            precision,
            ..
        } => {
            schema.nodes[position].kind = Kind::Primitive(schema.leaves.len());
            schema.leaves.push(Leaf {
                node: position,
                root: 0,
                path: Vec::new(),
                physical: physical_type(*stored_physical),
                type_length: *type_length,
                precision: *precision,
                scale: *scale,
                converted,
                logical,
                max_definition: 0,
                max_repetition: 0,
            });
        }
        Type::GroupType { fields, .. } => {
            let children = fields.iter().map(|field| node(field, schema)).collect();
            schema.nodes[position].kind = Kind::Group(children);
        }
    }
    position
}

/// An encoding by the name parquet 55 gives it.
fn encoding(name: &str) -> Encoding {
    use serde::{de::IntoDeserializer, Deserialize};
    let name: serde::de::value::StrDeserializer<'_, serde::de::value::Error> =
        name.into_deserializer();
    Encoding::deserialize(name).unwrap()
}

/// A codec by the name parquet 55 gives it, without its level.
fn codec(name: &str) -> Codec {
    use serde::{de::IntoDeserializer, Deserialize};
    let name = name.split('(').next().unwrap();
    let name: serde::de::value::StrDeserializer<'_, serde::de::value::Error> =
        name.into_deserializer();
    Codec::deserialize(name).unwrap()
}

/// Both decoders' reading of the admitted footer of `bytes`; `None` where
/// framing refuses the file, which neither decoder then sees.
fn footers(bytes: &[u8]) -> Option<(Option<Footer>, Option<Footer>)> {
    let frame = framing::admit(bytes).ok()?;
    let upstream = row_groups::decode(frame.metadata, &frame.footer)
        .ok()
        .map(|metadata| upstream(&metadata));
    let native = footer::decode(frame.metadata).ok();
    Some((upstream, native))
}

/// The most mutations of one file's page headers, and of its page bytes,
/// the harness makes, evenly spread over all of them.
const PAGE_MUTATIONS: usize = 60;

/// One-field mutations of the page headers of an admitted file that keep
/// each header's length, and one-byte changes of its pages, the integers'
/// turn starting at `turn`.
fn page_mutations(bytes: &[u8], mut turn: usize) -> Vec<Vec<u8>> {
    let Ok(frame) = framing::admit(bytes) else {
        return Vec::new();
    };
    let mut headers = Vec::new();
    for page in &frame.pages {
        let held = &bytes[page.header.clone()];
        let tree = Reader { bytes: held, at: 0 }.value(12);
        let mut sites = Vec::new();
        tree.sites(&mut Vec::new(), &mut turn, &mut sites);
        for site in &sites {
            let mut header = Vec::new();
            tree.mutated(site).write(&mut header);
            if header.len() == held.len() && header != held {
                headers.push((page.header.start, header));
            }
        }
    }
    let stride = headers.len().div_ceil(PAGE_MUTATIONS).max(1);
    let mut files: Vec<_> = headers
        .into_iter()
        .step_by(stride)
        .map(|(at, header)| {
            let mut file = bytes.to_vec();
            file[at..at + header.len()].copy_from_slice(&header);
            file
        })
        .collect();
    let body: Vec<usize> = frame
        .pages
        .iter()
        .flat_map(|page| page.body.clone())
        .collect();
    let stride = body.len().div_ceil(PAGE_MUTATIONS).max(1);
    for (index, &at) in body.iter().step_by(stride).enumerate() {
        let mut file = bytes.to_vec();
        file[at] ^= [0x01, 0x80, 0xff, 0x10][(turn + index) % 4];
        files.push(file);
    }
    files
}

/// A physical type as the reader names it (`format`).
fn physical_type(stored: parquet::basic::Type) -> Physical {
    use parquet::basic::Type as P;
    match stored {
        P::BOOLEAN => Physical::Boolean,
        P::INT32 => Physical::Int32,
        P::INT64 => Physical::Int64,
        P::INT96 => Physical::Int96,
        P::FLOAT => Physical::Float,
        P::DOUBLE => Physical::Double,
        P::BYTE_ARRAY => Physical::ByteArray,
        P::FIXED_LEN_BYTE_ARRAY => Physical::FixedLenByteArray,
    }
}

/// A converted type as the reader names it (`format`), by its name.
fn converted_type(stored: parquet::basic::ConvertedType) -> Converted {
    use Converted as C;
    let name = format!("{stored:?}");
    [
        C::None,
        C::Utf8,
        C::Map,
        C::MapKeyValue,
        C::List,
        C::Enum,
        C::Decimal,
        C::Date,
        C::TimeMillis,
        C::TimeMicros,
        C::TimestampMillis,
        C::TimestampMicros,
        C::Uint8,
        C::Uint16,
        C::Uint32,
        C::Uint64,
        C::Int8,
        C::Int16,
        C::Int32,
        C::Int64,
        C::Json,
        C::Bson,
        C::Interval,
    ]
    .into_iter()
    .find(|converted| converted.name() == name)
    .unwrap()
}

/// A logical type as the reader names it (`format`).
fn logical_type(logical: &parquet::basic::LogicalType) -> Logical {
    use parquet::basic::{LogicalType as L, TimeUnit as U};
    let unit = |unit: &U| match unit {
        U::MILLIS(_) => TimeUnit::Millis,
        U::MICROS(_) => TimeUnit::Micros,
        U::NANOS(_) => TimeUnit::Nanos,
    };
    match logical {
        L::String => Logical::String,
        L::Map => Logical::Map,
        L::List => Logical::List,
        L::Enum => Logical::Enum,
        L::Decimal { scale, precision } => Logical::Decimal {
            scale: *scale,
            precision: *precision,
        },
        L::Date => Logical::Date,
        L::Time {
            is_adjusted_to_u_t_c,
            unit: time,
        } => Logical::Time {
            utc: *is_adjusted_to_u_t_c,
            unit: unit(time),
        },
        L::Timestamp {
            is_adjusted_to_u_t_c,
            unit: time,
        } => Logical::Timestamp {
            utc: *is_adjusted_to_u_t_c,
            unit: unit(time),
        },
        L::Integer {
            bit_width,
            is_signed,
        } => Logical::Integer {
            bits: *bit_width,
            signed: *is_signed,
        },
        L::Unknown => Logical::Unknown,
        L::Json => Logical::Json,
        L::Bson => Logical::Bson,
        L::Uuid => Logical::Uuid,
        L::Float16 => Logical::Float16,
    }
}

/// parquet 55's decoders, which the reader read with until #3878.
const UPSTREAM: Decoders = Decoders {
    footer: |frame| {
        Ok(upstream(&row_groups::decode(
            frame.metadata,
            &frame.footer,
        )?))
    },
    arrow: upstream_arrow,
    columns: upstream_columns,
};

/// The Arrow schema parquet 55 gave the admitted footer, with the file's
/// embedded schema, or in its place the schema as the reader held it when
/// it named a later type (`metadata::Hint`).
fn upstream_arrow(
    frame: &framing::Framing<'_>,
    _: &Footer,
    hint: Option<&metadata::Hint>,
) -> Result<arrow_schema::Schema, Error> {
    use parquet::{
        arrow::{encode_arrow_schema, parquet_to_arrow_schema, ARROW_SCHEMA_META_KEY},
        file::metadata::KeyValue,
    };
    let metadata = row_groups::decode(frame.metadata, &frame.footer)?;
    let schema = metadata.file_metadata().schema_descr();
    let held = hint.filter(|hint| !hint.later.is_empty());
    physical::native(|| match held {
        Some(held) => {
            let hint = encode_arrow_schema(&held.schema);
            let held = vec![KeyValue::new(ARROW_SCHEMA_META_KEY.to_string(), hint)];
            parquet_to_arrow_schema(schema, Some(&held))
        }
        None => parquet_to_arrow_schema(schema, metadata.file_metadata().key_value_metadata()),
    })
}

/// The stored columns parquet 55's column reader read from the row groups
/// `groups` of the held file `file`, each page header presented as the
/// footer is.
fn upstream_columns(
    file: &[u8],
    _: &[framing::Page],
    footer: &Footer,
    keep: &[bool],
    arrow: &arrow_schema::Schema,
    groups: std::ops::Range<usize>,
) -> Result<Vec<Column>, Error> {
    let frame = framing::admit_partitioned(file, Some(std::num::NonZeroUsize::MIN))?;
    let metadata = row_groups::decode(frame.metadata, &frame.footer)?;
    let source = presented_pages(file, &frame)?;
    physical::read(source, &metadata, footer, keep, arrow, groups)
}

/// Copy the held file for parquet 55's page reader, presenting each page
/// header as the footer is presented to its metadata decoder. The
/// presentation keeps each header's length, so it is written in place. The
/// reader has already refused a presented file whose pages share a byte
/// (`reader::refuse_shared_presented_pages`).
fn presented_pages(bytes: &[u8], frame: &framing::Framing<'_>) -> Result<bytes::Bytes, Error> {
    let mut file: Option<Vec<u8>> = None;
    for page in &frame.pages {
        let header = page.header.clone();
        if let std::borrow::Cow::Owned(native) = compact::native_metadata(&bytes[header.clone()])? {
            file.get_or_insert_with(|| bytes.to_vec())[header].copy_from_slice(&native);
        }
    }
    for opaque in &frame.opaque {
        file.get_or_insert_with(|| bytes.to_vec())[opaque.clone()].fill(0);
    }
    Ok(file.map_or_else(|| bytes::Bytes::copy_from_slice(bytes), bytes::Bytes::from))
}

/// The columns' signatures as the reader spelled them from parquet 55's
/// footer.
fn upstream_signature(bytes: &[u8]) -> Result<Vec<ColumnSignature>, Error> {
    let frame = framing::admit(bytes)?;
    let metadata = row_groups::decode(frame.metadata, &frame.footer)?;
    Ok(metadata
        .file_metadata()
        .schema_descr()
        .columns()
        .iter()
        .map(|column| ColumnSignature {
            path: column.path().string(),
            physical_type: format!("{:?}", column.physical_type()),
            logical_type: column.logical_type().map(|logical| format!("{logical:?}")),
            converted_type: format!("{:?}", column.converted_type()),
            precision: column.type_precision(),
            scale: column.type_scale(),
            max_definition_level: column.max_def_level(),
            max_repetition_level: column.max_rep_level(),
        })
        .collect())
}

/// What the reader reads of a file with one set of decoders.
#[derive(Debug, PartialEq)]
struct Reading {
    signature: Result<Vec<ColumnSignature>, Error>,
    /// The table schema of an ordinary and of an ODM read, from the footer.
    schemas: [Result<TableSchema, Error>; 2],
    /// The table of an ordinary and of an ODM read, or its first finding.
    tables: [Result<(TableSchema, Vec<RecordBatch>), Error>; 2],
}

fn reading(
    bytes: &[u8],
    decoders: Decoders,
    signature: Result<Vec<ColumnSignature>, Error>,
) -> Reading {
    Reading {
        signature,
        schemas: [false, true].map(|odm| reader::table_schema(bytes, odm, decoders)),
        tables: [false, true]
            .map(|odm| reader::whole(bytes, odm, decoders).map(|read| (read.schema, read.batches))),
    }
}

/// How the inputs and their mutations compared: the footers both decoders
/// read and both refused, the tables both read and both refused, and each
/// input they read otherwise.
#[derive(Default)]
struct Compared {
    footers: [usize; 2],
    tables: [usize; 2],
    divergent: Vec<String>,
}

impl Compared {
    /// Compare one input, recording a panic as a divergence of that input:
    /// parquet 55's own are contained (`physical::native`), so one here is
    /// the native reader's.
    fn compare(&mut self, label: &str, bytes: &[u8]) {
        let compared = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.compare_readings(label, bytes);
        }));
        if compared.is_err() {
            self.divergent
                .push(format!("{label}: the native reader panicked"));
        }
    }

    fn compare_readings(&mut self, label: &str, bytes: &[u8]) {
        match footers(bytes) {
            None => {}
            Some((Some(upstream), Some(native))) if upstream == native => self.footers[0] += 1,
            Some((None, None)) => self.footers[1] += 1,
            Some((upstream, native)) => {
                return self.divergent.push(format!(
                    "{label}: the footer: {}",
                    difference(&upstream, &native)
                ));
            }
        }
        let native = reading(bytes, NATIVE, reader::signature(bytes));
        let upstream = reading(bytes, UPSTREAM, upstream_signature(bytes));
        if native != upstream {
            return self
                .divergent
                .push(format!("{label}: {}", difference(&upstream, &native)));
        }
        for table in &native.tables {
            self.tables[usize::from(table.is_err())] += 1;
        }
    }

    fn add(mut self, other: Self) -> Self {
        for index in 0..2 {
            self.footers[index] += other.footers[index];
            self.tables[index] += other.tables[index];
        }
        self.divergent.extend(other.divergent);
        self
    }
}

#[test]
fn every_input_reads_alike_natively_and_through_parquet_55() {
    let inputs = inputs();
    // The inputs are independent, so they are compared on every thread the
    // machine offers.
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let compared = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|thread| {
                let inputs = &inputs;
                scope.spawn(move || {
                    let mut compared = Compared::default();
                    for (input, (label, bytes)) in
                        inputs.iter().enumerate().skip(thread).step_by(threads)
                    {
                        compared.compare(label, bytes);
                        let footers = mutated(bytes, input);
                        let pages = page_mutations(bytes, input);
                        for (index, mutation) in footers.iter().chain(&pages).enumerate() {
                            compared.compare(&format!("{label}, mutation {index}"), mutation);
                        }
                    }
                    compared
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .fold(Compared::default(), Compared::add)
    });
    let Compared {
        footers,
        tables,
        divergent,
    } = compared;
    assert!(
        divergent.is_empty(),
        "{} inputs read otherwise; the first: {:#?}",
        divergent.len(),
        &divergent[..divergent.len().min(5)]
    );
    // Both outcomes occur, so the comparison covers each.
    assert!(
        footers.iter().chain(&tables).all(|&count| count > 0),
        "footers read and refused {footers:?}, tables {tables:?}"
    );
}

/// Where two readings of one input first differ, in their debug forms.
fn difference<T: std::fmt::Debug>(upstream: &T, native: &T) -> String {
    let (upstream, native) = (format!("{upstream:?}"), format!("{native:?}"));
    let at = upstream
        .bytes()
        .zip(native.bytes())
        .position(|(left, right)| left != right)
        .unwrap_or(upstream.len().min(native.len()));
    let from = at.saturating_sub(120);
    let context = |text: &str| {
        text.get(from..(at + 80).min(text.len()))
            .unwrap_or("")
            .to_owned()
    };
    format!(
        "reads otherwise: parquet 55 `{}`, native `{}`",
        context(&upstream),
        context(&native)
    )
}

/// The expectation the harness holds beside agreement: the host fixtures'
/// invalid Parquet files, for which `parquet-containment` states one
/// `source_parquet_invalid` row, are refused as malformed by both decoders.
/// The INT96 statistic is refused with the footer; the page that two row
/// groups name, whose header parquet 55 was shown otherwise, where the read
/// compares the pages (`reader::refuse_shared_presented_pages`).
#[test]
fn both_decoders_refuse_the_invalid_host_fixture_files() {
    let truth =
        crate::json_wire::decode(include_str!("../../../../tests/host-fixtures/cases.json"))
            .unwrap();
    for name in [
        "long_int96_statistic_parquet",
        "shared_presented_page_parquet",
    ] {
        let hex = truth["families"]["parquet-containment"][name]["hex"]
            .as_str()
            .unwrap();
        let bytes = unhex(hex);
        for (decoders, label) in [(NATIVE, "native"), (UPSTREAM, "parquet 55")] {
            assert_eq!(
                reader::whole(&bytes, false, decoders).map(|_| ()),
                Err(Error::Malformed),
                "{name}, {label}"
            );
        }
    }
}
