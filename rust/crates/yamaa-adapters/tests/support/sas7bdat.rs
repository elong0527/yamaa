//! SAS7BDAT files assembled at test time from the documented physical layout.
//!
//! The file header, page header, subheader pointers and the row-size,
//! column-size, column-text, column-name and column-attributes subheaders are
//! written from their published field offsets. Rows are written byte by byte,
//! and compressed rows use plain RLE copy commands or RDC literals and runs.
//! Nothing here reads a file with yamaa or ReadStat, so expected values stay
//! independent of the reader.

pub const META: u16 = 0x0000;
pub const DATA: u16 = 0x0100;
pub const MIX: u16 = 0x0200;
pub const AMD: u16 = 0x0400;
pub const COMP: u16 = 0x9000;
/// The page-type flag of a data or mix page that holds records deleted in place.
pub const DELETED_ROWS: u16 = 0x0080;

pub const ROW_SIZE: u32 = 0xF7F7_F7F7;
pub const COLUMN_SIZE: u32 = 0xF6F6_F6F6;
pub const COLUMN_TEXT: u32 = 0xFFFF_FFFD;
pub const COLUMN_NAME: u32 = 0xFFFF_FFFF;
pub const COLUMN_ATTRS: u32 = 0xFFFF_FFFC;

/// The documented SAS7BDAT magic number.
pub const MAGIC: [u8; 32] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc2, 0xea, 0x81, 0x60,
    0xb3, 0x14, 0x11, 0xcf, 0xbd, 0x92, 0x08, 0x00, 0x09, 0xc7, 0x31, 0x8c, 0x18, 0x1f, 0x10, 0x11,
];

/// Word size and byte order of one file.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub wide: bool,
    pub little: bool,
}

impl Layout {
    pub const LE32: Self = Self {
        wide: false,
        little: true,
    };
    pub const LE64: Self = Self {
        wide: true,
        little: true,
    };
    pub const BE32: Self = Self {
        wide: false,
        little: false,
    };
    pub const BE64: Self = Self {
        wide: true,
        little: false,
    };

    pub fn word(self) -> usize {
        if self.wide {
            8
        } else {
            4
        }
    }
    pub fn page_header_len(self) -> usize {
        if self.wide {
            40
        } else {
            24
        }
    }
    pub fn pointer_len(self) -> usize {
        if self.wide {
            24
        } else {
            12
        }
    }

    /// `value` in `width` bytes of this file's byte order.
    pub fn bytes(self, value: u64, width: usize) -> Vec<u8> {
        let all = if self.little {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        };
        if self.little {
            all[..width].to_vec()
        } else {
            all[8 - width..].to_vec()
        }
    }
    pub fn put(self, target: &mut [u8], at: usize, value: u64, width: usize) {
        target[at..at + width].copy_from_slice(&self.bytes(value, width));
    }
    pub fn double(self, value: f64) -> [u8; 8] {
        if self.little {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        }
    }

    /// The signature that opens a metadata subheader. A 64-bit little-endian
    /// file stores the signature as one eight-byte integer. A 64-bit
    /// big-endian file writes `0xFFFFFFFF` before the four-byte signatures
    /// of that family, and leaves the four bytes after the row-size and
    /// column-size signatures unspecified; these carry non-zero bytes here.
    pub fn signature(self, value: u32) -> Vec<u8> {
        if !self.wide {
            return self.bytes(u64::from(value), 4);
        }
        let family = value & 0xFFFF_0000 == 0xFFFF_0000;
        if self.little {
            let wide = if family {
                0xFFFF_FFFF_0000_0000 | u64::from(value)
            } else {
                u64::from(value)
            };
            return self.bytes(wide, 8);
        }
        let mut bytes = if family {
            vec![0xFF; 4]
        } else {
            self.bytes(u64::from(value), 4)
        };
        bytes.extend(if family {
            self.bytes(u64::from(value), 4)
        } else {
            vec![0xFF, 0xFF, 0xFB, 0xFE]
        });
        bytes
    }

    fn padded(self, value: u32, entries: &[Vec<u8>]) -> Vec<u8> {
        // Signature, a two-byte remainder and six reserved bytes precede the
        // entries; twelve (64-bit) or eight (32-bit) reserved bytes follow.
        let signature = self.signature(value);
        let mut bytes = signature.clone();
        bytes.extend([0; 8]);
        for entry in entries {
            bytes.extend(entry);
        }
        bytes.extend(vec![0; if self.wide { 12 } else { 8 }]);
        let remainder = bytes.len() - 4 - 2 * signature.len();
        self.put(&mut bytes, signature.len(), remainder as u64, 2);
        bytes
    }

    /// One reference to a range of a column-text subheader.
    pub fn text_ref(self, reference: TextRef) -> Vec<u8> {
        let mut bytes = self.bytes(u64::from(reference.index), 2);
        bytes.extend(self.bytes(u64::from(reference.offset), 2));
        bytes.extend(self.bytes(u64::from(reference.length), 2));
        bytes
    }

    /// The row-size subheader. The record counts, row length and mix-page
    /// row count sit at fixed offsets; the compression literal is a text
    /// reference 118 bytes before the end.
    pub fn row_size(
        self,
        row_length: u64,
        total: u64,
        deleted: u64,
        mix_rows: u64,
        compression: TextRef,
    ) -> Vec<u8> {
        let length = if self.wide { 808 } else { 480 };
        let mut bytes = vec![0; length];
        let signature = self.signature(ROW_SIZE);
        bytes[..signature.len()].copy_from_slice(&signature);
        let (word, base) = (self.word(), if self.wide { 40 } else { 20 });
        self.put(&mut bytes, base, row_length, word);
        self.put(&mut bytes, base + word, total, word);
        self.put(&mut bytes, base + 2 * word, deleted, word);
        self.put(&mut bytes, if self.wide { 120 } else { 60 }, mix_rows, word);
        let reference = self.text_ref(compression);
        bytes[length - 118..length - 112].copy_from_slice(&reference);
        bytes
    }

    pub fn column_size(self, columns: u64) -> Vec<u8> {
        let mut bytes = self.signature(COLUMN_SIZE);
        bytes.extend(self.bytes(columns, self.word()));
        bytes.extend(vec![0; self.word()]);
        bytes
    }

    /// A column-text subheader whose text blob, measured from the end of the
    /// signature, is `blob`. Its first two bytes become the remainder.
    pub fn column_text(self, blob: &[u8]) -> Vec<u8> {
        let mut bytes = self.signature(COLUMN_TEXT);
        let signature = bytes.len();
        bytes.extend(blob);
        let remainder = bytes.len() - 4 - 2 * signature;
        self.put(&mut bytes, signature, remainder as u64, 2);
        bytes
    }

    pub fn column_name(self, names: &[TextRef]) -> Vec<u8> {
        let entries: Vec<_> = names
            .iter()
            .map(|name| {
                let mut entry = self.text_ref(*name);
                entry.extend([0, 0]);
                entry
            })
            .collect();
        self.padded(COLUMN_NAME, &entries)
    }

    /// One column-attributes entry per field: row offset, width and type.
    pub fn column_attrs(self, fields: &[Field]) -> Vec<u8> {
        let entries: Vec<_> = fields
            .iter()
            .map(|field| {
                let mut entry = self.bytes(field.offset, self.word());
                entry.extend(self.bytes(u64::from(field.width), 4));
                entry.extend([0, 0, field.kind as u8, 0]);
                entry
            })
            .collect();
        self.padded(COLUMN_ATTRS, &entries)
    }
}

/// A `(blob index, offset, length)` reference into column text.
#[derive(Clone, Copy, Debug, Default)]
pub struct TextRef {
    pub index: u16,
    pub offset: u16,
    pub length: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Num = 1,
    Chr = 2,
}

/// One field's physical row slot.
#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub kind: Kind,
    pub offset: u64,
    pub width: u32,
}

/// One cell of a row, in the order of the fields.
pub enum Cell<'a> {
    Num(f64),
    Text(&'a [u8]),
    /// Exact stored bytes, for numbers given by their bit pattern.
    Bytes(&'a [u8]),
}

/// A subheader pointer and, for a stored subheader, its bytes.
#[derive(Clone, Debug)]
pub enum Pointer {
    Stored {
        bytes: Vec<u8>,
        compression: u8,
        data: bool,
    },
    /// Offset and length written as given: references and truncated pointers.
    Raw {
        offset: u64,
        length: u64,
        compression: u8,
        data: bool,
    },
}

impl Pointer {
    pub fn metadata(bytes: Vec<u8>) -> Self {
        Self::Stored {
            bytes,
            compression: 0,
            data: true,
        }
    }
    /// A row SAS kept uncompressed in a compressed dataset.
    pub fn raw_row(bytes: Vec<u8>) -> Self {
        Self::Stored {
            bytes,
            compression: 0,
            data: true,
        }
    }
    /// A row compressed with RLE copy commands.
    pub fn rle_row(row: &[u8]) -> Self {
        Self::Stored {
            bytes: rle(row),
            compression: 4,
            data: true,
        }
    }
    /// Stored bytes with an explicit compression code.
    pub fn coded(bytes: Vec<u8>, compression: u8) -> Self {
        Self::Stored {
            bytes,
            compression,
            data: true,
        }
    }
    /// A record moved to subheader `index` (zero-based) of page `page`.
    pub fn reference(page: u64, index: u64) -> Self {
        Self::Raw {
            offset: page + 1,
            length: index + 1,
            compression: 3,
            data: true,
        }
    }
    pub fn truncated() -> Self {
        Self::Raw {
            offset: 0,
            length: 0,
            compression: 1,
            data: false,
        }
    }
}

/// RLE copy commands only: control byte `0x80 | (n - 1)` copies the next
/// `n` bytes, for `n` from 1 to 16.
pub fn rle(row: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for chunk in row.chunks(16) {
        bytes.push(0x80 | (chunk.len() as u8 - 1));
        bytes.extend(chunk);
    }
    bytes
}

/// RDC with literal bytes and runs of three to eighteen equal bytes. A 16-bit
/// control word, most significant bit first, describes the next sixteen items:
/// a clear bit is one literal byte and a set bit is a run, stored as its length
/// minus three and the repeated byte.
pub fn rdc(row: &[u8]) -> Vec<u8> {
    enum Item {
        Literal(u8),
        Run(u8, u8),
    }
    let mut items = Vec::new();
    let mut at = 0;
    while at < row.len() {
        let run = row[at..]
            .iter()
            .take(18)
            .take_while(|&&byte| byte == row[at])
            .count();
        if run >= 3 {
            items.push(Item::Run(run as u8 - 3, row[at]));
            at += run;
        } else {
            items.push(Item::Literal(row[at]));
            at += 1;
        }
    }
    let mut bytes = Vec::new();
    for group in items.chunks(16) {
        let (mut control, mut payload) = (0u16, Vec::new());
        for (index, item) in group.iter().enumerate() {
            match *item {
                Item::Literal(byte) => payload.push(byte),
                Item::Run(length, byte) => {
                    control |= 0x8000 >> index;
                    payload.extend([length, byte]);
                }
            }
        }
        bytes.extend(control.to_be_bytes());
        bytes.extend(payload);
    }
    bytes
}

/// One page: its type, subheader pointers and data-area rows.
#[derive(Clone, Debug, Default)]
pub struct Page {
    pub kind: u16,
    pub pointers: Vec<Pointer>,
    pub rows: Vec<Vec<u8>>,
    /// One bit per data-area row, most significant bit first. `Some` adds
    /// the deleted-rows flag to the page type.
    pub deleted: Option<Vec<bool>>,
    /// Overrides the page header's free-space field.
    pub free_space: Option<u64>,
    /// Overrides the page header's block count.
    pub block_count: Option<u16>,
}

impl Page {
    pub fn new(kind: u16, pointers: Vec<Pointer>, rows: Vec<Vec<u8>>) -> Self {
        Self {
            kind,
            pointers,
            rows,
            ..Self::default()
        }
    }
}

/// A dataset's fields, counts and file framing.
#[derive(Clone, Debug)]
pub struct Dataset {
    pub layout: Layout,
    pub header_len: usize,
    pub page_len: usize,
    pub magic: [u8; 32],
    pub encoding: u8,
    pub release: [u8; 8],
    pub fields: Vec<Field>,
    pub row_length: u64,
    pub compression: Option<[u8; 8]>,
    pub total: u64,
    pub deleted: u64,
    pub mix_rows: u64,
}

impl Dataset {
    /// Fields packed in order, with a 4096-byte page.
    pub fn new(layout: Layout, fields: &[(&str, Kind, u32)]) -> Self {
        let mut offset = 0;
        let fields: Vec<_> = fields
            .iter()
            .map(|&(name, kind, width)| {
                let field = Field {
                    name: name.into(),
                    kind,
                    offset,
                    width,
                };
                offset += u64::from(width);
                field
            })
            .collect();
        Self {
            layout,
            header_len: 1024,
            page_len: 4096,
            magic: MAGIC,
            encoding: 20,
            release: *b"9.0401M6",
            fields,
            row_length: offset,
            compression: None,
            total: 0,
            deleted: 0,
            mix_rows: 0,
        }
    }

    /// The column-text blob: remainder, reserved bytes, the compression
    /// literal when present, then each field name. Returns the blob, the
    /// compression reference and one reference per name.
    pub fn text(&self) -> (Vec<u8>, TextRef, Vec<TextRef>) {
        let mut blob = vec![0; 8];
        let mut add = |text: &[u8]| {
            let reference = TextRef {
                index: 0,
                offset: blob.len() as u16,
                length: text.len() as u16,
            };
            blob.extend(text);
            blob.resize(blob.len().div_ceil(4) * 4, 0);
            reference
        };
        let compression = self
            .compression
            .map(|literal| add(&literal))
            .unwrap_or_default();
        let names = self
            .fields
            .iter()
            .map(|field| add(field.name.as_bytes()))
            .collect();
        (blob, compression, names)
    }

    /// Row size, column size, column text, column names and column attributes.
    pub fn metadata(&self) -> Vec<Pointer> {
        let layout = self.layout;
        let (blob, compression, names) = self.text();
        vec![
            Pointer::metadata(layout.row_size(
                self.row_length,
                self.total,
                self.deleted,
                self.mix_rows,
                compression,
            )),
            Pointer::metadata(layout.column_size(self.fields.len() as u64)),
            Pointer::metadata(layout.column_text(&blob)),
            Pointer::metadata(layout.column_name(&names)),
            Pointer::metadata(layout.column_attrs(&self.fields)),
        ]
    }

    pub fn row(&self, cells: &[Cell<'_>]) -> Vec<u8> {
        assert_eq!(cells.len(), self.fields.len());
        let mut row = vec![0; self.row_length as usize];
        for (field, cell) in self.fields.iter().zip(cells) {
            let slot = &mut row[field.offset as usize..][..field.width as usize];
            match cell {
                Cell::Num(value) => {
                    let bytes = self.layout.double(*value);
                    // Shortened numbers keep their most significant bytes.
                    if self.layout.little {
                        slot.copy_from_slice(&bytes[8 - slot.len()..]);
                    } else {
                        slot.copy_from_slice(&bytes[..slot.len()]);
                    }
                }
                Cell::Text(text) => {
                    slot.fill(b' ');
                    slot[..text.len()].copy_from_slice(text);
                }
                Cell::Bytes(bytes) => slot.copy_from_slice(bytes),
            }
        }
        row
    }

    pub fn page(&self, page: &Page) -> Vec<u8> {
        let layout = self.layout;
        let header = layout.page_header_len();
        let mut bytes = vec![0; self.page_len];
        let table_end = header + page.pointers.len() * layout.pointer_len();
        let mut lowest = self.page_len;
        for (index, pointer) in page.pointers.iter().enumerate() {
            let (offset, length, compression, data) = match pointer {
                Pointer::Stored {
                    bytes: content,
                    compression,
                    data,
                } => {
                    lowest -= content.len();
                    lowest -= lowest % 8;
                    bytes[lowest..lowest + content.len()].copy_from_slice(content);
                    (lowest as u64, content.len() as u64, *compression, *data)
                }
                Pointer::Raw {
                    offset,
                    length,
                    compression,
                    data,
                } => (*offset, *length, *compression, *data),
            };
            let at = header + index * layout.pointer_len();
            let word = layout.word();
            layout.put(&mut bytes, at, offset, word);
            layout.put(&mut bytes, at + word, length, word);
            bytes[at + 2 * word] = compression;
            bytes[at + 2 * word + 1] = u8::from(data);
        }
        let mut start = if page.kind & 0x0F00 == DATA {
            header
        } else {
            table_end
        };
        if start % 8 == 4 {
            start += 4;
        }
        let rows_end = start + page.rows.len() * self.row_length as usize;
        for (index, row) in page.rows.iter().enumerate() {
            let at = start + index * self.row_length as usize;
            bytes[at..at + row.len()].copy_from_slice(row);
        }
        let mut free_end = lowest;
        let mut kind = page.kind;
        if let Some(deleted) = &page.deleted {
            kind |= DELETED_ROWS;
            let bitmap_len = deleted.len().div_ceil(8);
            free_end = (lowest - bitmap_len.div_ceil(8) * 8) / 8 * 8;
            for (index, &gone) in deleted.iter().enumerate() {
                if gone {
                    bytes[free_end + index / 8] |= 0x80 >> (index % 8);
                }
            }
        }
        assert!(rows_end <= free_end && table_end <= free_end);
        // Free space runs from the last row, or the pointer table on a page
        // without rows, to the deleted-row bitmap or the stored subheaders.
        let used = if page.rows.is_empty() {
            table_end
        } else {
            rows_end
        };
        let free_space = page.free_space.unwrap_or((free_end - used) as u64);
        let field = if layout.wide { 24 } else { 12 };
        layout.put(&mut bytes, field, free_space, layout.word());
        layout.put(&mut bytes, header - 8, u64::from(kind), 2);
        let blocks = page
            .block_count
            .unwrap_or((page.pointers.len() + page.rows.len()) as u16);
        layout.put(&mut bytes, header - 6, u64::from(blocks), 2);
        layout.put(&mut bytes, header - 4, page.pointers.len() as u64, 2);
        bytes
    }

    /// The file header: magic number, alignment, byte order, encoding,
    /// header and page sizes, page count and release.
    pub fn header(&self, page_count: usize) -> Vec<u8> {
        let layout = self.layout;
        let mut bytes = vec![0; self.header_len];
        bytes[..32].copy_from_slice(&self.magic);
        // 0x33 marks the 64-bit word size and the four alignment bytes that
        // precede the timestamps in 64-bit files; 0x22 marks neither.
        let aligned = if layout.wide { 0x33 } else { 0x22 };
        bytes[32] = aligned;
        bytes[35] = aligned;
        bytes[37] = u8::from(layout.little);
        bytes[39] = b'1';
        bytes[70] = self.encoding;
        bytes[84..92].copy_from_slice(b"DATA    ");
        bytes[92..124].fill(b' ');
        bytes[92..98].copy_from_slice(b"BUILT ");
        let pad = if layout.wide { 4 } else { 0 };
        let creation = layout.double(1_700_000_000.0);
        bytes[164 + pad..172 + pad].copy_from_slice(&creation);
        bytes[172 + pad..180 + pad].copy_from_slice(&creation);
        layout.put(&mut bytes, 196 + pad, self.header_len as u64, 4);
        layout.put(&mut bytes, 200 + pad, self.page_len as u64, 4);
        layout.put(&mut bytes, 204 + pad, page_count as u64, layout.word());
        // The release, then the host name, padded with NUL bytes as SAS does.
        let release = self.release_offset();
        bytes[release..release + 8].copy_from_slice(&self.release);
        bytes[release + 8..release + 15].copy_from_slice(b"X64_SRV");
        bytes
    }

    /// Where the eight-byte release field starts in the file header.
    pub fn release_offset(&self) -> usize {
        let pad = if self.layout.wide { 4 } else { 0 };
        204 + pad + self.layout.word() + 8
    }

    pub fn file(&self, pages: &[Page]) -> Vec<u8> {
        let mut bytes = self.header(pages.len());
        for page in pages {
            bytes.extend(self.page(page));
        }
        bytes
    }
}

/// Independent UTF-8 fixture: ID 1, a concentration unit with a micro sign.
/// The header records code 20; the character slot holds literal UTF-8 bytes.
pub fn utf8_concentration() -> Vec<u8> {
    let mut dataset = Dataset::new(
        Layout::LE32,
        &[("ID", Kind::Num, 8), ("UNIT", Kind::Chr, 12)],
    );
    dataset.total = 1;
    dataset.mix_rows = 1;
    let row = dataset.row(&[Cell::Num(1.0), Cell::Text("\u{b5}mol/L".as_bytes())]);
    dataset.file(&[Page::new(MIX, dataset.metadata(), vec![row])])
}
