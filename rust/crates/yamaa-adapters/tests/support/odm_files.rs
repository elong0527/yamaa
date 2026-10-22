//! ODM test inputs written at test time: scratch files, TAR archives with
//! ustar, GNU and PAX headers, gzip streams, and readers for the result
//! table. No binary fixture is committed.
use arrow_array::{Array, Int64Array, StringArray};
use std::io::Write;
use yamaa_adapters::public_table::PublicTable;

/// Compress bytes as one gzip member.
pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

const BLOCK: usize = 512;

/// One header block; `magic` selects POSIX ustar or GNU layout.
#[derive(Clone)]
pub struct Header {
    pub name: Vec<u8>,
    pub prefix: Vec<u8>,
    pub typeflag: u8,
    pub size: u64,
    pub link: Vec<u8>,
    pub gnu: bool,
    pub sparse_extended: bool,
}

impl Header {
    pub fn new(name: &str, typeflag: u8, size: u64) -> Self {
        Self {
            name: name.as_bytes().to_vec(),
            prefix: Vec::new(),
            typeflag,
            size,
            link: Vec::new(),
            gnu: false,
            sparse_extended: false,
        }
    }

    fn octal(field: &mut [u8], value: u64) {
        let text = format!("{value:0width$o}", width = field.len() - 1);
        assert!(
            text.len() < field.len(),
            "value too large for the octal field"
        );
        field[..text.len()].copy_from_slice(text.as_bytes());
        field[text.len()] = 0;
    }

    pub fn encode(&self) -> [u8; BLOCK] {
        let mut block = [0_u8; BLOCK];
        assert!(self.name.len() <= 100 && self.prefix.len() <= 155 && self.link.len() <= 100);
        block[..self.name.len()].copy_from_slice(&self.name);
        Self::octal(&mut block[100..108], 0o644);
        Self::octal(&mut block[108..116], 0);
        Self::octal(&mut block[116..124], 0);
        Self::octal(&mut block[124..136], self.size);
        Self::octal(&mut block[136..148], 1_767_225_600);
        block[156] = self.typeflag;
        block[157..157 + self.link.len()].copy_from_slice(&self.link);
        if self.gnu {
            block[257..265].copy_from_slice(b"ustar  \0");
        } else {
            block[257..265].copy_from_slice(b"ustar\x0000");
            block[345..345 + self.prefix.len()].copy_from_slice(&self.prefix);
        }
        block[265..270].copy_from_slice(b"yamaa");
        block[297..302].copy_from_slice(b"yamaa");
        if self.sparse_extended {
            block[482] = 1;
        }
        block[148..156].copy_from_slice(b"        ");
        let checksum: u64 = block.iter().map(|&byte| u64::from(byte)).sum();
        let text = format!("{checksum:06o}\0 ");
        block[148..156].copy_from_slice(text.as_bytes());
        block
    }
}

fn padding(size: usize) -> usize {
    size.div_ceil(BLOCK) * BLOCK - size
}

/// TAR bytes built member by member.
#[derive(Default)]
pub struct Tar {
    bytes: Vec<u8>,
}

impl Tar {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append one header and its data, padded to the block size.
    pub fn entry(mut self, header: &Header, data: &[u8]) -> Self {
        self.bytes.extend_from_slice(&header.encode());
        self.bytes.extend_from_slice(data);
        self.bytes
            .extend(std::iter::repeat_n(0, padding(data.len())));
        self
    }

    /// A regular POSIX ustar file.
    pub fn file(self, name: &str, data: &[u8]) -> Self {
        self.entry(&Header::new(name, b'0', data.len() as u64), data)
    }

    /// A ustar file whose path is split into prefix and name fields.
    pub fn prefixed(self, prefix: &str, name: &str, data: &[u8]) -> Self {
        let mut header = Header::new(name, b'0', data.len() as u64);
        header.prefix = prefix.as_bytes().to_vec();
        self.entry(&header, data)
    }

    /// A member with no data, such as a directory or link.
    pub fn special(self, name: &str, typeflag: u8, link: &str) -> Self {
        let mut header = Header::new(name, typeflag, 0);
        header.link = link.as_bytes().to_vec();
        self.entry(&header, &[])
    }

    /// PAX extended records followed by a regular member.
    pub fn pax(self, records: &[(&str, &str)], name: &str, data: &[u8]) -> Self {
        let mut payload = Vec::new();
        for (key, value) in records {
            let body = format!(" {key}={value}\n");
            let mut length = body.len() + 1;
            while format!("{length}{body}").len() != length {
                length += 1;
            }
            payload.extend_from_slice(format!("{length}{body}").as_bytes());
        }
        let header = Header::new("PaxHeaders/member", b'x', payload.len() as u64);
        self.entry(&header, &payload).file(name, data)
    }

    /// A PAX global header, which applies to no single member.
    pub fn pax_global(self, records: &[(&str, &str)]) -> Self {
        let mut payload = Vec::new();
        for (key, value) in records {
            let body = format!(" {key}={value}\n");
            let mut length = body.len() + 1;
            while format!("{length}{body}").len() != length {
                length += 1;
            }
            payload.extend_from_slice(format!("{length}{body}").as_bytes());
        }
        let header = Header::new("pax_global_header", b'g', payload.len() as u64);
        self.entry(&header, &payload)
    }

    /// A GNU long-name header followed by a GNU regular member.
    pub fn gnu_long_name(self, name: &str, data: &[u8]) -> Self {
        let mut payload = name.as_bytes().to_vec();
        payload.push(0);
        let mut long = Header::new("././@LongLink", b'L', payload.len() as u64);
        long.gnu = true;
        let mut member = Header::new(&name[..name.len().min(100)], b'0', data.len() as u64);
        member.gnu = true;
        self.entry(&long, &payload).entry(&member, data)
    }

    /// Raw bytes, for damaged archives.
    pub fn raw(mut self, bytes: &[u8]) -> Self {
        self.bytes.extend_from_slice(bytes);
        self
    }

    /// The archive with its two end-of-archive zero blocks.
    pub fn finish(mut self) -> Vec<u8> {
        self.bytes.extend(std::iter::repeat_n(0, 2 * BLOCK));
        self.bytes
    }

    /// The archive without an end-of-archive marker.
    pub fn unterminated(self) -> Vec<u8> {
        self.bytes
    }
}

/// Every value of a text column across batches.
pub fn column(table: &PublicTable, name: &str) -> Vec<Option<String>> {
    let index = table.schema().index_of(name).unwrap();
    table
        .batches()
        .iter()
        .flat_map(|batch| {
            let values = batch
                .column(index)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            (0..values.len())
                .map(|row| (!values.is_null(row)).then(|| values.value(row).to_owned()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The one value of a text column in the given row.
pub fn cell(table: &PublicTable, name: &str, row: usize) -> Option<String> {
    column(table, name).swap_remove(row)
}

/// Every `SourceOrdinal` across batches.
pub fn ordinals(table: &PublicTable) -> Vec<i64> {
    let index = table.schema().index_of("SourceOrdinal").unwrap();
    table
        .batches()
        .iter()
        .flat_map(|batch| {
            batch
                .column(index)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .values()
                .to_vec()
        })
        .collect()
}
