//! Table contracts: SAS source, source diagnostics and typed CSV.
//!
//! Each module names the seam its tests prove. What a build reads from a
//! typed CSV input is a case of the application suite's
//! `fixtures/diagnostics/typed_csv_sources.json`
//! (#3798, #3778 group 3).
mod table {
    #[cfg(feature = "readstat")]
    mod sas_source {
        //! Binary source truth is authored independently of the candidate parser.
        //!
        //! Seam, for every test here: the SAS7BDAT and Transport codecs over
        //! binary files built, patched or read from `fixtures/sas/` in the test,
        //! and the codec's own error causes, which no case's text files state.
        use std::path::Path;
        use yamaa_adapters::sas_source::{self, Error};
        use yamaa_core::{
            specification::{SourceDeclaration, SourceProfile},
            table::{TableAccess, ValueRef},
            value::ColumnType,
        };

        fn bytes(name: &str) -> Vec<u8> {
            std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/sas")
                    .join(name),
            )
            .unwrap()
        }
        fn declaration(name: &str) -> SourceDeclaration {
            SourceDeclaration {
                name: "SRC".into(),
                path: name.into(),
                types: vec![],
                profile: SourceProfile::from_path(name).unwrap(),
                encoding: Some("windows-1252".into()),
                byte_order_mark: false,
                empty_string_present: false,
                ordinal: None,
            }
        }
        fn float(value: f64) -> ValueRef<'static> {
            ValueRef::Float(yamaa_core::value::FiniteFloat::new(value).unwrap())
        }
        /// Every cell of a decoded table, row by row.
        fn cells(table: &yamaa_adapters::column_table::ColumnTable) -> Vec<Vec<ValueRef<'_>>> {
            (0..table.row_count())
                .map(|row| {
                    (0..table.schema().columns().len())
                        .map(|column| table.cell(row, column).unwrap())
                        .collect()
                })
                .collect()
        }
        /// A file built at test time, read as `built.sas7bdat`.
        fn read_built(bytes: &[u8]) -> Result<yamaa_adapters::column_table::ColumnTable, Error> {
            sas_source::parse(bytes, &declaration("built.sas7bdat"))
        }
        #[test]
        fn raw_rows_that_begin_like_a_subheader_signature_stay_rows_in_64_bit_files() {
            // A compressed dataset keeps a row it could not compress as an
            // uncompressed subheader. In a 64-bit little-endian file each row
            // below opens with the four bytes of a subheader signature, but
            // only a whole eight-byte signature opens a subheader.
            use crate::support::sas7bdat::{Cell, Dataset, Kind, Layout, Page, Pointer, META};
            let mut dataset =
                Dataset::new(Layout::LE64, &[("X", Kind::Num, 8), ("T", Kind::Chr, 8)]);
            dataset.compression = Some(*b"SASYZCRL");
            let leading: [[u8; 4]; 9] = [
                [0xFF, 0xFF, 0xFF, 0xFF], // column names
                [0xFD, 0xFF, 0xFF, 0xFF], // column text
                [0xFE, 0xFF, 0xFF, 0xFF], // column list
                [0xF8, 0xFF, 0xFF, 0xFF], // the unnamed 0xFFFFFFF8 family
                [0xFC, 0xFF, 0xFF, 0xFF], // column attributes
                [0x00, 0xFC, 0xFF, 0xFF], // subheader counts
                [0xFE, 0xFB, 0xFF, 0xFF], // column format
                [0xF7, 0xF7, 0xF7, 0xF7], // row size
                [0xF6, 0xF6, 0xF6, 0xF6], // column size
            ];
            let values: Vec<f64> = leading
                .iter()
                .map(|bytes| {
                    f64::from_le_bytes([
                        bytes[0], bytes[1], bytes[2], bytes[3], 0xFF, 0xFF, 0xEF, 0x3F,
                    ])
                })
                .collect();
            assert_eq!(values[0], 0.9999999999999999);
            dataset.total = 1 + values.len() as u64;
            let mut pointers = dataset.metadata();
            pointers.push(Pointer::rle_row(
                &dataset.row(&[Cell::Num(1.5), Cell::Text(b"rle")]),
            ));
            let labels: Vec<String> = (0..values.len()).map(|row| format!("raw{row}")).collect();
            for (value, label) in values.iter().zip(&labels) {
                pointers.push(Pointer::raw_row(
                    dataset.row(&[Cell::Num(*value), Cell::Text(label.as_bytes())]),
                ));
            }
            pointers.push(Pointer::truncated());
            let bytes = dataset.file(&[Page::new(META, pointers, vec![])]);
            let table = read_built(&bytes).unwrap();
            let mut expected = vec![vec![float(1.5), ValueRef::Str("rle")]];
            for (value, label) in values.iter().zip(&labels) {
                expected.push(vec![float(*value), ValueRef::Str(label)]);
            }
            assert_eq!(cells(&table), expected);
        }
        #[test]
        fn compressed_row_after_uncompressed_rows_gets_its_own_row_buffer() {
            // Columns are submitted at the first row. Here that row is in a
            // mix page's data area, so no compressed row buffer exists yet
            // when an RLE row follows on the next page. The reader must
            // allocate it rather than decompress through a null buffer.
            use crate::support::sas7bdat::{Cell, Dataset, Kind, Layout, Page, Pointer, META, MIX};
            let mut dataset =
                Dataset::new(Layout::LE32, &[("X", Kind::Num, 8), ("T", Kind::Chr, 8)]);
            dataset.compression = Some(*b"SASYZCRL");
            dataset.total = 2;
            dataset.mix_rows = 1;
            let first = dataset.row(&[Cell::Num(4.0), Cell::Text(b"mix")]);
            let second = dataset.row(&[Cell::Num(-0.5), Cell::Text(b"rle")]);
            let bytes = dataset.file(&[
                Page::new(MIX, dataset.metadata(), vec![first]),
                Page::new(META, vec![Pointer::rle_row(&second)], vec![]),
            ]);
            assert_eq!(
                cells(&read_built(&bytes).unwrap()),
                [
                    [float(4.0), ValueRef::Str("mix")],
                    [float(-0.5), ValueRef::Str("rle")]
                ]
            );
        }
        /// Where the record after a repeated row-size subheader is stored.
        #[derive(Clone, Copy)]
        enum Follow {
            Nothing,
            InPlace,
            Moved,
        }
        /// A compressed dataset of `X` and `T` whose first record is stored for
        /// a row length of 16. A second row-size subheader then gives `later`
        /// as the row length, and the second record, stored for that length,
        /// follows it compressed with RDC or RLE, in place or as a moved record.
        fn restated_row_length(
            layout: crate::support::sas7bdat::Layout,
            rdc: bool,
            later: u64,
            follow: Follow,
        ) -> Vec<u8> {
            use crate::support::sas7bdat::{
                rdc as rdc_bytes, rle, Cell, Dataset, Kind, Page, Pointer, META,
            };
            let mut dataset = Dataset::new(layout, &[("X", Kind::Num, 8), ("T", Kind::Chr, 8)]);
            dataset.compression = Some(if rdc { *b"SASYZCR2" } else { *b"SASYZCRL" });
            dataset.total = if matches!(follow, Follow::Nothing) {
                1
            } else {
                2
            };
            let (_, compression, _) = dataset.text();
            let compress = |row: &[u8]| if rdc { rdc_bytes(row) } else { rle(row) };
            let mut pointers = dataset.metadata();
            let first = dataset.row(&[Cell::Num(1.0), Cell::Text(b"one")]);
            pointers.push(Pointer::coded(compress(&first), 4));
            let restated = layout.row_size(later, dataset.total, 0, 0, compression);
            pointers.push(Pointer::metadata(restated));
            let mut second = dataset.row(&[Cell::Num(2.0), Cell::Text(b"two")]);
            second.resize(later as usize, 0);
            let mut moved = None;
            match follow {
                Follow::Nothing => {}
                Follow::InPlace => pointers.push(Pointer::coded(compress(&second), 4)),
                Follow::Moved => {
                    pointers.push(Pointer::reference(1, 0));
                    let stored = Pointer::coded(compress(&second), 0x06);
                    moved = Some(Page::new(META, vec![stored], vec![]));
                }
            }
            let mut pages = vec![Page::new(META, pointers, vec![])];
            pages.extend(moved);
            dataset.file(&pages)
        }
        #[test]
        fn a_row_size_subheader_cannot_change_the_row_length_once_columns_are_submitted() {
            // The submitted columns fit the row length in force at the first
            // record, and the compressed row buffer holds that many bytes. A
            // later row-size subheader may repeat the row length. One that
            // changes it is refused even when no record follows, whether it
            // shortens the row or lengthens it.
            use crate::support::sas7bdat::Layout;
            for layout in [Layout::LE32, Layout::LE64, Layout::BE32, Layout::BE64] {
                for rdc in [false, true] {
                    for follow in [Follow::InPlace, Follow::Moved] {
                        let bytes = restated_row_length(layout, rdc, 16, follow);
                        assert_eq!(
                            cells(&read_built(&bytes).unwrap()),
                            [
                                [float(1.0), ValueRef::Str("one")],
                                [float(2.0), ValueRef::Str("two")]
                            ],
                            "{layout:?} rdc {rdc}"
                        );
                    }
                    for later in [8, 15, 17, 4096] {
                        let bytes = restated_row_length(layout, rdc, later, Follow::Nothing);
                        assert!(
                            matches!(read_built(&bytes), Err(Error::Invalid("malformed"))),
                            "{layout:?} rdc {rdc} later {later}"
                        );
                    }
                }
            }
        }
        #[test]
        fn compressed_records_after_a_changed_row_length_are_refused_before_they_are_written() {
            // A record stored for the longer row length would decompress past
            // a buffer allocated for the first. Each way of storing a
            // compressed record is refused: RLE and RDC, in place and moved.
            use crate::support::sas7bdat::Layout;
            for layout in [Layout::LE32, Layout::LE64, Layout::BE32, Layout::BE64] {
                for rdc in [false, true] {
                    for follow in [Follow::InPlace, Follow::Moved] {
                        for later in [17, 32, 200] {
                            let bytes = restated_row_length(layout, rdc, later, follow);
                            assert!(
                                matches!(read_built(&bytes), Err(Error::Invalid("malformed"))),
                                "{layout:?} rdc {rdc} moved {} later {later}",
                                matches!(follow, Follow::Moved)
                            );
                        }
                    }
                }
            }
        }
        #[test]
        fn release_field_without_a_terminator_still_parses() {
            // The header's 120-byte release block holds no NUL byte at all.
            // The reader copies the eight-byte release into a terminated
            // buffer before scanning it, so the scan ends at the eighth byte.
            // Scanning the field in place would read on into the next field,
            // which a sanitizer reports. Without one, the second file shows
            // it: its release is "9.      ", whose number sits in the next
            // field, "0401M6". The eight bytes name no minor version, so the
            // file is malformed. A scan that passes the eighth byte skips the
            // blanks, reads release 9.0401M6 and accepts it.
            use crate::support::sas7bdat::{Cell, Dataset, Kind, Layout, Page, MIX};
            let mut dataset = Dataset::new(Layout::LE32, &[("X", Kind::Num, 8)]);
            dataset.total = 1;
            dataset.mix_rows = 1;
            let row = dataset.row(&[Cell::Num(7.0)]);
            let mut bytes = dataset.file(&[Page::new(MIX, dataset.metadata(), vec![row])]);
            let release = dataset.release_offset();
            bytes[release + 8..release + 120].fill(b' ');
            assert!(!bytes[release..release + 120].contains(&0));
            assert_eq!(cells(&read_built(&bytes).unwrap()), [[float(7.0)]]);
            bytes[release..release + 8].copy_from_slice(b"9.      ");
            bytes[release + 8..release + 14].copy_from_slice(b"0401M6");
            assert!(!bytes[release..release + 120].contains(&0));
            assert!(matches!(
                read_built(&bytes),
                Err(Error::Invalid("malformed"))
            ));
        }
        #[test]
        fn a_page_count_with_its_high_bit_set_is_refused() {
            // A 64-bit header holds its page count as an unsigned integer.
            // ReadStat kept it in a signed field, so a count with its high bit
            // set passed the upper bound, and the check that the pages fill
            // the file multiplied it by the page size: a signed overflow.
            // 0xff00000000000001 times 4096 wraps to 4096, the one page of
            // this file, so that check passed on the wrapped value. Fuzzing
            // found it. Only a sanitizer reports the overflow; its minimal
            // input is a regression input of the sas7bdat fuzz target.
            use crate::support::sas7bdat::{Cell, Dataset, Kind, Layout, Page, MIX};
            let mut dataset = Dataset::new(Layout::LE64, &[("X", Kind::Num, 8)]);
            dataset.total = 1;
            dataset.mix_rows = 1;
            let row = dataset.row(&[Cell::Num(7.0)]);
            let mut bytes = dataset.file(&[Page::new(MIX, dataset.metadata(), vec![row])]);
            assert_eq!(cells(&read_built(&bytes).unwrap()), [[float(7.0)]]);
            // The page count follows the header and page sizes, at 208.
            assert_eq!(&bytes[208..216], &1u64.to_le_bytes());
            bytes[208..216].copy_from_slice(&0xff00_0000_0000_0001_u64.to_le_bytes());
            assert!(matches!(
                read_built(&bytes),
                Err(Error::Invalid("malformed"))
            ));
        }
        #[test]
        fn zero_width_character_column_reads_as_empty_text() {
            // A zero-width character column stores no bytes, so every value
            // is the zero-length text that `empty_string` interprets.
            use crate::support::sas7bdat::{Cell, Dataset, Kind, Layout, Page, MIX};
            let mut dataset = Dataset::new(
                Layout::LE32,
                &[
                    ("X", Kind::Num, 8),
                    ("Z", Kind::Chr, 0),
                    ("T", Kind::Chr, 4),
                ],
            );
            dataset.total = 2;
            dataset.mix_rows = 2;
            let rows = vec![
                dataset.row(&[Cell::Num(1.0), Cell::Text(b""), Cell::Text(b"ab")]),
                dataset.row(&[Cell::Num(2.0), Cell::Text(b""), Cell::Text(b"cd")]),
            ];
            let bytes = dataset.file(&[Page::new(MIX, dataset.metadata(), rows)]);
            assert_eq!(
                cells(&read_built(&bytes).unwrap()),
                [
                    [float(1.0), ValueRef::Missing, ValueRef::Str("ab")],
                    [float(2.0), ValueRef::Missing, ValueRef::Str("cd")]
                ]
            );
            let mut source = declaration("built.sas7bdat");
            source.empty_string_present = true;
            let table = sas_source::parse(&bytes, &source).unwrap();
            assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Str(""));
        }
        #[test]
        fn columns_without_attributes_or_names_at_their_first_record_fail_as_malformed() {
            // Two numeric columns, X and Y. A column that no attributes
            // subheader has described would read as a zero-width character
            // column, so each case below must fail rather than yield an
            // empty Y: Y's attributes never arrive, Y's name never arrives,
            // or Y's attributes arrive, as a numeric column of width zero,
            // only after the first record.
            use crate::support::sas7bdat::{
                Cell, Dataset, Field, Kind, Layout, Page, Pointer, META, MIX,
            };
            let mut dataset =
                Dataset::new(Layout::LE32, &[("X", Kind::Num, 8), ("Y", Kind::Num, 8)]);
            dataset.total = 2;
            dataset.mix_rows = 2;
            let layout = dataset.layout;
            let (blob, compression, names) = dataset.text();
            let file = |names: &[_], fields: &[Field], later: Vec<Pointer>, mix_rows: u64| {
                let metadata = vec![
                    Pointer::metadata(layout.row_size(16, 2, 0, mix_rows, compression)),
                    Pointer::metadata(layout.column_size(2)),
                    Pointer::metadata(layout.column_text(&blob)),
                    Pointer::metadata(layout.column_name(names)),
                    Pointer::metadata(layout.column_attrs(fields)),
                ];
                let row = dataset.row(&[Cell::Num(1.0), Cell::Num(2.0)]);
                let rows = vec![row; mix_rows as usize];
                let mut pages = vec![Page::new(MIX, metadata, rows)];
                if !later.is_empty() {
                    pages.push(Page::new(META, later, vec![]));
                }
                dataset.file(&pages)
            };
            let complete = file(&names, &dataset.fields, vec![], 2);
            let table = read_built(&complete).unwrap();
            assert_eq!(cells(&table), [[float(1.0), float(2.0)]; 2]);
            let row = dataset.row(&[Cell::Num(1.0), Cell::Num(2.0)]);
            let changed = Field {
                name: "Y".into(),
                kind: Kind::Num,
                offset: 8,
                width: 0,
            };
            let late = vec![
                Pointer::metadata(layout.column_attrs(&[changed])),
                Pointer::raw_row(row),
            ];
            for bytes in [
                file(&names, &dataset.fields[..1], vec![], 2),
                file(&names[..1], &dataset.fields, vec![], 2),
                file(&names, &dataset.fields[..1], late, 1),
            ] {
                assert!(matches!(
                    read_built(&bytes),
                    Err(Error::Invalid("malformed"))
                ));
            }
        }
        /// An entry-less column subheader, `empty`, shortened by `cut` bytes
        /// with its remainder field set to agree with the shorter length.
        fn shortened(
            layout: crate::support::sas7bdat::Layout,
            empty: &[u8],
            cut: usize,
        ) -> Vec<u8> {
            let mut short = empty[..empty.len() - cut].to_vec();
            let remainder = short.len() - 4 - 2 * layout.word();
            layout.put(&mut short, layout.word(), remainder as u64, 2);
            short
        }
        #[test]
        fn column_subheaders_shorter_than_their_fixed_part_are_refused() {
            // A column-name or column-attributes subheader holds a fixed part,
            // 28 bytes in a 64-bit file and 20 in any other, then its
            // entries. Below that fixed part the entry count would wrap, so
            // the subheader is malformed even when its remainder field agrees
            // with its length. One of exactly the fixed part holds no entries
            // and is valid. Each short subheader is stored before the valid
            // subheader of its kind.
            use crate::support::sas7bdat::{Cell, Dataset, Kind, Layout, Page, Pointer, MIX};
            for layout in [Layout::LE32, Layout::LE64, Layout::BE32, Layout::BE64] {
                let mut dataset = Dataset::new(layout, &[("X", Kind::Num, 8)]);
                dataset.total = 1;
                dataset.mix_rows = 1;
                let row = dataset.row(&[Cell::Num(7.0)]);
                let file = |at: usize, extra: Vec<u8>| {
                    let mut pointers = dataset.metadata();
                    pointers.insert(at, Pointer::metadata(extra));
                    dataset.file(&[Page::new(MIX, pointers, vec![row.clone()])])
                };
                // Names are the fourth subheader and attributes the fifth.
                for (at, empty) in [(3, layout.column_name(&[])), (4, layout.column_attrs(&[]))] {
                    let table = read_built(&file(at, empty.clone())).unwrap();
                    assert_eq!(cells(&table), [[float(7.0)]], "{layout:?} {at}");
                    for cut in 1..=8 {
                        let bytes = file(at, shortened(layout, &empty, cut));
                        assert!(
                            matches!(read_built(&bytes), Err(Error::Invalid("malformed"))),
                            "{layout:?} {at} {cut}"
                        );
                    }
                }
            }
        }
        #[test]
        fn a_short_attributes_subheader_cannot_rewrite_a_column_after_the_first_record() {
            // After the first record the two columns are submitted. A later
            // attributes subheader describes a further slot, so it changes
            // nothing the reader uses. A short one must not move the count of
            // attribute slots back, which would let the next subheader
            // overwrite Y's width and read the second record differently.
            use crate::support::sas7bdat::{
                Cell, Dataset, Field, Kind, Layout, Page, Pointer, META, MIX,
            };
            for layout in [Layout::LE32, Layout::LE64, Layout::BE32, Layout::BE64] {
                let mut dataset = Dataset::new(layout, &[("X", Kind::Num, 8), ("Y", Kind::Num, 8)]);
                dataset.total = 2;
                dataset.mix_rows = 1;
                let row = dataset.row(&[Cell::Num(1.0), Cell::Num(2.0)]);
                let narrowed = Field {
                    name: "Y".into(),
                    kind: Kind::Num,
                    offset: 8,
                    width: 4,
                };
                let file = |short: Option<Vec<u8>>| {
                    let mut later: Vec<Pointer> =
                        short.into_iter().map(Pointer::metadata).collect();
                    later.push(Pointer::metadata(
                        layout.column_attrs(std::slice::from_ref(&narrowed)),
                    ));
                    later.push(Pointer::raw_row(row.clone()));
                    dataset.file(&[
                        Page::new(MIX, dataset.metadata(), vec![row.clone()]),
                        Page::new(META, later, vec![]),
                    ])
                };
                let table = read_built(&file(None)).unwrap();
                assert_eq!(cells(&table), [[float(1.0), float(2.0)]; 2], "{layout:?}");
                let empty = layout.column_attrs(&[]);
                assert!(
                    matches!(
                        read_built(&file(Some(shortened(layout, &empty, 1)))),
                        Err(Error::Invalid("malformed"))
                    ),
                    "{layout:?}"
                );
            }
        }
        #[test]
        fn empty_dataset_reads_column_metadata_from_every_page() {
            // No records, and the name and attributes of the second column
            // are on the second metadata page.
            use crate::support::sas7bdat::{Dataset, Kind, Layout, Page, Pointer, META};
            let dataset = Dataset::new(Layout::LE64, &[("X", Kind::Num, 8), ("Y", Kind::Num, 8)]);
            let layout = dataset.layout;
            let (blob, compression, names) = dataset.text();
            let first = vec![
                Pointer::metadata(layout.row_size(16, 0, 0, 0, compression)),
                Pointer::metadata(layout.column_size(2)),
                Pointer::metadata(layout.column_text(&blob)),
                Pointer::metadata(layout.column_name(&names[..1])),
                Pointer::metadata(layout.column_attrs(&dataset.fields[..1])),
            ];
            let second = vec![
                Pointer::metadata(layout.column_name(&names[1..])),
                Pointer::metadata(layout.column_attrs(&dataset.fields[1..])),
            ];
            let bytes = dataset.file(&[
                Page::new(META, first, vec![]),
                Page::new(META, second, vec![]),
            ]);
            let table = read_built(&bytes).unwrap();
            assert_eq!(table.row_count(), 0);
            assert_eq!(
                table
                    .schema()
                    .columns()
                    .iter()
                    .map(|column| (column.name.as_str(), column.kind))
                    .collect::<Vec<_>>(),
                [("X", ColumnType::Float), ("Y", ColumnType::Float)]
            );
        }
        #[test]
        fn sas_7_release_strings_and_tk_revisions_are_accepted() {
            // The header's release field reads major.minor, a revision tag
            // and a revision. Release 8 and 9 tags are M, J or T (as in TK);
            // other majors, such as SAS 7, may carry any tag.
            use crate::support::sas7bdat::{Cell, Dataset, Kind, Layout, Page, MAGIC, MIX};
            let file = |release: &[u8; 8], magic: [u8; 32]| {
                let mut dataset = Dataset::new(Layout::LE32, &[("X", Kind::Num, 8)]);
                dataset.total = 1;
                dataset.mix_rows = 1;
                dataset.release = *release;
                dataset.magic = magic;
                let row = dataset.row(&[Cell::Num(3.0)]);
                dataset.file(&[Page::new(MIX, dataset.metadata(), vec![row])])
            };
            for release in [b"9.0401M6", b"7.00.00B", b"9.0401J0", b"9.0401TK"] {
                let table = read_built(&file(release, MAGIC)).unwrap();
                assert_eq!(cells(&table), [[float(3.0)]], "{release:?}");
            }
            for release in [b"9.0401X0", b"9.0401MX", b"9.0401TX", b"9-0401M0"] {
                assert!(
                    matches!(
                        read_built(&file(release, MAGIC)),
                        Err(Error::Invalid("malformed"))
                    ),
                    "{release:?}"
                );
            }
            // Some writers zero every magic byte but the last four.
            let mut alternative = [0; 32];
            alternative[28..].copy_from_slice(&MAGIC[28..]);
            let table = read_built(&file(b"9.0401M6", alternative)).unwrap();
            assert_eq!(cells(&table), [[float(3.0)]]);
            let mut unknown = MAGIC;
            unknown[31] ^= 1;
            assert!(matches!(
                read_built(&file(b"9.0401M6", unknown)),
                Err(Error::Invalid("malformed"))
            ));
        }
        #[test]
        fn big_endian_64_bit_signatures_keep_their_four_byte_layout() {
            // In a 64-bit big-endian file the row-size signature is followed
            // by four unspecified bytes, and only 0xFFFFFFFF opens the
            // 0xFFFF... family. Rows that open with another member of that
            // family stay rows. Their leading bytes make each double a NaN,
            // which SAS reads as a special missing value.
            use crate::support::sas7bdat::{Cell, Dataset, Kind, Layout, Page, Pointer, META};
            let mut dataset =
                Dataset::new(Layout::BE64, &[("X", Kind::Num, 8), ("T", Kind::Chr, 8)]);
            dataset.compression = Some(*b"SASYZCRL");
            let leading: [[u8; 4]; 6] = [
                [0xFF, 0xFF, 0xFF, 0xFD], // column text
                [0xFF, 0xFF, 0xFF, 0xFE], // column list
                [0xFF, 0xFF, 0xFF, 0xF8], // the unnamed 0xFFFFFFF8 family
                [0xFF, 0xFF, 0xFF, 0xFC], // column attributes
                [0xFF, 0xFF, 0xFC, 0x00], // subheader counts
                [0xFF, 0xFF, 0xFB, 0xFE], // column format
            ];
            dataset.total = 1 + leading.len() as u64;
            let mut pointers = dataset.metadata();
            pointers.push(Pointer::rle_row(
                &dataset.row(&[Cell::Num(-2.25), Cell::Text(b"rle")]),
            ));
            let labels: Vec<String> = (0..leading.len()).map(|row| format!("raw{row}")).collect();
            for (bytes, label) in leading.iter().zip(&labels) {
                let number = [bytes[0], bytes[1], bytes[2], bytes[3], 0, 0, 0, 0];
                pointers.push(Pointer::raw_row(
                    dataset.row(&[Cell::Bytes(&number), Cell::Text(label.as_bytes())]),
                ));
            }
            pointers.push(Pointer::truncated());
            let bytes = dataset.file(&[Page::new(META, pointers, vec![])]);
            let table = read_built(&bytes).unwrap();
            let mut expected = vec![vec![float(-2.25), ValueRef::Str("rle")]];
            for label in &labels {
                expected.push(vec![ValueRef::Missing, ValueRef::Str(label)]);
            }
            assert_eq!(cells(&table), expected);
        }
        /// Every cell equals the independent pandas CSV truth for `records`.
        fn assert_compressed_truth(
            table: &yamaa_adapters::column_table::ColumnTable,
            names: &[String],
            records: &[Vec<Option<String>>],
        ) {
            assert_eq!(table.row_count(), records.len());
            assert_eq!(table.schema().columns().len(), 100);
            for (column, field) in table.schema().columns().iter().enumerate() {
                assert_eq!(field.name, names[column]);
                assert_eq!(
                    field.kind,
                    if column % 4 == 1 {
                        ColumnType::Str
                    } else {
                        ColumnType::Float
                    }
                );
                for (row, record) in records.iter().enumerate() {
                    let actual = table.cell(row, column).unwrap();
                    match record[column].as_deref() {
                        None => assert_eq!(actual, ValueRef::Missing),
                        Some(text) if column % 4 == 1 => {
                            assert_eq!(actual, ValueRef::Str(text))
                        }
                        Some(number) => assert_eq!(actual, float(number.parse().unwrap())),
                    }
                }
            }
        }
        #[test]
        fn rle_and_rdc_compression_equal_independent_upstream_csv_truth() {
            let expected =
                yamaa_adapters::csv_source::parse(&bytes("compressed-truth.csv")).unwrap();
            for file in ["rle.sas7bdat", "rdc.sas7bdat"] {
                let table = sas_source::parse(&bytes(file), &declaration(file)).unwrap();
                assert_compressed_truth(&table, &expected.names, &expected.records);
            }
        }
        #[test]
        fn malformed_compressed_controls_fail_closed() {
            // Independently identified first compressed-row command offsets from the
            // upstream pandas SAS corruption controls, retained in the source inventory.
            for (file, offset, byte) in [
                ("rle.sas7bdat", 0x10000 + 55229, 0x10),
                ("rdc.sas7bdat", 118170, 184),
            ] {
                let mut data = bytes(file);
                data[offset] = byte;
                assert!(
                    sas_source::parse(&data, &declaration(file)).is_err(),
                    "{file}"
                );
            }
        }
        #[test]
        fn rdc_overlapping_back_reference_propagates_newly_emitted_bytes() {
            let file = "rdc-big-endian.sas7bdat";
            let mut data = bytes(file);
            assert_eq!(&data[120922..120924], &[0x8d, 0x00]);
            data[120922..120924].copy_from_slice(&[0x81, 0x00]);
            let table = sas_source::parse(&data, &declaration(file)).unwrap();
            let independent_original = 0.621f64.to_be_bytes();
            let mut repeated = [0; 8];
            repeated[..4].copy_from_slice(&independent_original[4..]);
            repeated[4..].copy_from_slice(&independent_original[4..]);
            assert_eq!(
                table.cell(0, 10).unwrap(),
                ValueRef::Float(
                    yamaa_core::value::FiniteFloat::new(f64::from_be_bytes(repeated)).unwrap()
                )
            );
        }
        #[test]
        fn both_xport_versions_preserve_order_padding_unicode_and_default_float() {
            for (file, text) in [("v5.xpt", "TEXT"), ("v8.xpt", "LONG_TEXT_NAME")] {
                let mut source = declaration(file);
                let table = sas_source::parse(&bytes(file), &source).unwrap();
                assert_eq!(table.row_count(), 3);
                assert_eq!(table.schema().columns()[0].kind, ColumnType::Float);
                assert_eq!(table.schema().columns()[1].name, text);
                assert_eq!(
                    table.cell(0, 0).unwrap(),
                    ValueRef::Float(yamaa_core::value::FiniteFloat::new(2.0).unwrap())
                );
                assert_eq!(
                    table.cell(1, 0).unwrap(),
                    ValueRef::Float(yamaa_core::value::FiniteFloat::new(1.0).unwrap())
                );
                assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Str(" café"));
                assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Missing);
                assert_eq!(table.cell(2, 1).unwrap(), ValueRef::Str("."));
                assert_eq!(table.cell(1, 2).unwrap(), ValueRef::Missing);
                // DATE9-like physical formats above do not infer the column type.
                assert_eq!(table.schema().columns()[3].kind, ColumnType::Float);
                source.empty_string_present = true;
                source.types = vec![
                    ("ID".into(), ColumnType::Int),
                    ("DAY".into(), ColumnType::Date),
                    ("STAMP".into(), ColumnType::DateTime),
                ];
                let table = sas_source::parse(&bytes(file), &source).unwrap();
                assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(2));
                assert_eq!(table.cell(1, 1).unwrap(), ValueRef::Str(""));
                assert_eq!(
                    table.cell(0, 3).unwrap(),
                    ValueRef::Date("1960-01-01".parse().unwrap())
                );
                assert_eq!(
                    table.cell(1, 3).unwrap(),
                    ValueRef::Date("1970-01-01".parse().unwrap())
                );
                assert_eq!(
                    table.cell(1, 4).unwrap(),
                    ValueRef::DateTime("1959-12-31T23:59:59".parse().unwrap())
                );
            }
        }
        const TEXT_FIELDS: [(&str, u16); 2] = [("A", 8), ("B", 8)];
        /// A record of text then two blank records, padded with two more
        /// blank 16-byte records to a whole 80-byte record.
        fn trailing_blanks() -> Vec<u8> {
            crate::support::xport::records(&TEXT_FIELDS, &[&["x", "y"], &["", ""], &["", ""]])
        }
        #[test]
        fn version_8_observation_count_keeps_trailing_blank_records() {
            // OBSV8 counts the observations in fifteen characters from byte
            // 48 of its header record. With a count, records made entirely
            // of blanks are data rather than padding.
            use crate::support::xport::transport;
            let mut source = declaration("built.xpt");
            for count in [format!("{:15}", 3), "000000000000003".into()] {
                let bytes = transport(8, &TEXT_FIELDS, &count, &trailing_blanks());
                let table = sas_source::parse(&bytes, &source).unwrap();
                assert_eq!(
                    cells(&table),
                    [
                        [ValueRef::Str("x"), ValueRef::Str("y")],
                        [ValueRef::Missing, ValueRef::Missing],
                        [ValueRef::Missing, ValueRef::Missing]
                    ],
                    "{count:?}"
                );
            }
            source.empty_string_present = true;
            let bytes = transport(8, &TEXT_FIELDS, &format!("{:15}", 3), &trailing_blanks());
            let table = sas_source::parse(&bytes, &source).unwrap();
            assert_eq!(table.row_count(), 3);
            assert_eq!(table.cell(2, 1).unwrap(), ValueRef::Str(""));
        }
        #[test]
        fn version_8_observation_count_must_be_decimal_digits_between_blanks() {
            // The count is fifteen characters from byte 48 of the OBSV8
            // header. Blanks may precede or follow its digits. A sign, a
            // letter, a point or a blank inside the digits makes the header
            // malformed; reading such a count as unknown would instead let
            // the trailing blank records pass as padding.
            use crate::support::xport::transport;
            let source = declaration("built.xpt");
            for count in [
                format!("{:15}", 3),
                format!("{:<15}", 3),
                format!("{:^15}", 3),
                "000000000000003".to_string(),
            ] {
                let bytes = transport(8, &TEXT_FIELDS, &count, &trailing_blanks());
                let table = sas_source::parse(&bytes, &source).unwrap();
                assert_eq!(table.row_count(), 3, "{count:?}");
            }
            for count in [
                "-00000000000003",
                "+00000000000003",
                "00000000000003x",
                "0000000000000x3",
                "000000.00000003",
                "000000 00000003",
                "               ",
            ] {
                let bytes = transport(8, &TEXT_FIELDS, count, &trailing_blanks());
                assert!(
                    matches!(
                        sas_source::parse(&bytes, &source),
                        Err(Error::Invalid("malformed"))
                    ),
                    "{count:?}"
                );
            }
        }
        #[test]
        fn transport_without_a_usable_count_treats_trailing_blank_records_as_padding() {
            // Version 5 carries no count, and a zero version 8 count is
            // unknown. Trailing blank records narrower than one 80-byte
            // record may be padding, so they are not data.
            use crate::support::xport::transport;
            for (version, observations) in [(5, "0".repeat(30)), (8, "0".repeat(15))] {
                let bytes = transport(version, &TEXT_FIELDS, &observations, &trailing_blanks());
                let table = sas_source::parse(&bytes, &declaration("built.xpt")).unwrap();
                assert_eq!(
                    cells(&table),
                    [[ValueRef::Str("x"), ValueRef::Str("y")]],
                    "{version} {observations:?}"
                );
            }
        }
        #[test]
        fn trailing_blank_records_of_any_width_are_padding_only_without_a_count() {
            // Without a count, in version 5 and with a zero version 8 count,
            // a trailing record made entirely of blanks may be padding at any
            // width, including a whole 80-byte record or wider, so it is not
            // data. A version 8 count keeps every record it counts.
            use crate::support::xport::{records, transport};
            for width in [40, 80, 120] {
                let fields = [("A", width)];
                let data = records(&fields, &[&["x"], &[""], &[""]]);
                for (version, observations) in [(5, "0".repeat(30)), (8, "0".repeat(15))] {
                    let bytes = transport(version, &fields, &observations, &data);
                    let table = sas_source::parse(&bytes, &declaration("built.xpt")).unwrap();
                    assert_eq!(cells(&table), [[ValueRef::Str("x")]], "{version} {width}");
                }
                let bytes = transport(8, &fields, &format!("{:15}", 3), &data);
                let table = sas_source::parse(&bytes, &declaration("built.xpt")).unwrap();
                assert_eq!(
                    cells(&table),
                    [
                        [ValueRef::Str("x")],
                        [ValueRef::Missing],
                        [ValueRef::Missing]
                    ],
                    "8 {width}"
                );
            }
        }
        #[test]
        fn version_8_count_that_disagrees_with_the_records_is_refused() {
            // Three text records fill 48 of 80 bytes; the blank padding holds
            // two more 16-byte records. A count of six exceeds every record
            // the file holds, and a count of one leaves text after it.
            use crate::support::xport::{records, transport};
            let data = records(&TEXT_FIELDS, &[&["x", "y"], &["p", "q"], &["r", "s"]]);
            for count in [6, 1] {
                let bytes = transport(8, &TEXT_FIELDS, &format!("{count:15}"), &data);
                assert!(
                    matches!(
                        sas_source::parse(&bytes, &declaration("built.xpt")),
                        Err(Error::Invalid("malformed"))
                    ),
                    "{count}"
                );
            }
            let bytes = transport(8, &TEXT_FIELDS, &format!("{:15}", 5), &data);
            let table = sas_source::parse(&bytes, &declaration("built.xpt")).unwrap();
            assert_eq!(table.row_count(), 5);
        }
        #[test]
        fn version_8_count_is_admitted_before_records_are_read() {
            use crate::support::xport::transport;
            let count = format!("{:15}", 10_000_001);
            let bytes = transport(8, &TEXT_FIELDS, &count, &trailing_blanks());
            assert!(matches!(
                sas_source::parse(&bytes, &declaration("built.xpt")),
                Err(Error::Limit {
                    resource: "table_rows",
                    actual: Some(10_000_001),
                    ..
                })
            ));
        }
        #[test]
        fn sas_dataset_numeric_date_formats_never_infer_and_epoch_narrowing_is_explicit() {
            let mut source = declaration("datetime.sas7bdat");
            let table = sas_source::parse(&bytes("datetime.sas7bdat"), &source).unwrap();
            assert_eq!(table.schema().columns().len(), 5);
            assert!(table
                .schema()
                .columns()
                .iter()
                .all(|column| column.kind == ColumnType::Float));
            source.types = vec![
                ("VAR1".into(), ColumnType::DateTime),
                ("VAR2".into(), ColumnType::Date),
                ("VAR3".into(), ColumnType::Date),
                ("VAR4".into(), ColumnType::Date),
                ("VAR5".into(), ColumnType::Int),
            ];
            let table = sas_source::parse(&bytes("datetime.sas7bdat"), &source).unwrap();
            assert_eq!(
                table.cell(0, 0).unwrap(),
                ValueRef::DateTime("2015-02-02T14:42:12".parse().unwrap())
            );
            for column in [1, 2, 3] {
                assert_eq!(
                    table.cell(0, column).unwrap(),
                    ValueRef::Date("2015-02-02".parse().unwrap())
                );
            }
            assert_eq!(table.cell(0, 4).unwrap(), ValueRef::Int(52932));
        }
        #[test]
        fn all_numeric_missing_tags_map_to_missing() {
            let table =
                sas_source::parse(&bytes("missing.xpt"), &declaration("missing.xpt")).unwrap();
            assert_eq!(table.row_count(), 28);
            for row in 0..28 {
                assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Missing);
            }
        }
        #[test]
        fn nonfinite_numeric_source_is_missing_before_every_authored_narrowing() {
            let mut payload = bytes("fractional.xpt");
            // Independent v5 layout: 640 header bytes, 160 NAMESTR/padding bytes,
            // 80 OBS bytes, then the first 8-byte numeric cell. ReadStat's documented
            // IBM non-finite sentinel maps to infinity, then core missing normalization.
            payload[880..888].copy_from_slice(&[0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
            for kind in [
                ColumnType::Float,
                ColumnType::Int,
                ColumnType::Date,
                ColumnType::DateTime,
            ] {
                let mut source = declaration("fractional.xpt");
                source.types = vec![("NUM".into(), kind)];
                let table = sas_source::parse(&payload, &source).unwrap();
                assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Missing);
            }
        }
        #[test]
        fn utf8_and_integer_boundaries_have_independent_exact_truth() {
            let mut source = declaration("utf8.xpt");
            source.encoding = Some("utf-8".into());
            let table = sas_source::parse(&bytes("utf8.xpt"), &source).unwrap();
            assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Str(" café"));
            let mut source = declaration("integer-bounds.xpt");
            source.types = vec![("NUM".into(), ColumnType::Int)];
            let table = sas_source::parse(&bytes("integer-bounds.xpt"), &source).unwrap();
            for (row, number) in [i64::MIN, 0, 9_007_199_254_740_992, i64::MAX - 1023]
                .into_iter()
                .enumerate()
            {
                assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Int(number));
            }
            source.path = "integer-overflow.xpt".into();
            assert!(matches!(
                sas_source::parse(&bytes("integer-overflow.xpt"), &source,),
                Err(Error::Typing(
                    yamaa_core::typed_csv::Error::FieldParse { .. }
                ))
            ));
        }
        #[test]
        fn unsupported_compression_has_a_specific_retained_cause() {
            let source = declaration("unsupported-compression.sas7bdat");
            assert!(matches!(
                sas_source::parse(&bytes(&source.path), &source),
                Err(Error::Invalid("compression"))
            ));
        }
        #[test]
        fn records_deleted_in_place_are_skipped_in_the_independent_haven_fixture() {
            // The haven fixture's mix page (type 0x0200 at 1040) holds four
            // 40-byte records from page offset 184, and its free-space field
            // (page offset 12, file offset 1036) is 2396. Its deleted-row
            // bitmap therefore starts at page offset 184 + 4 * 40 + 2396 =
            // 2740, file offset 3764, and holds zeros. The row-size subheader
            // at 4640 counts the records.
            let source = declaration("datetime.sas7bdat");
            let original = bytes(&source.path);
            assert_eq!(&original[1040..1042], &0x0200u16.to_le_bytes());
            assert_eq!(&original[1036..1040], &2396u32.to_le_bytes());
            assert_eq!(&original[3764..3772], &[0; 8]);
            assert_eq!(&original[4664..4672], &[4, 0, 0, 0, 0, 0, 0, 0]);
            // Literal physical binary64 truth of the four stored records.
            let stored = [
                [1_738_507_332.0, 20_121.0, 20_121.0, 20_121.0, 52_932.0],
                [1_704_190_463.0, 19_724.0, 19_724.0, 19_724.0, 36_863.0],
                [1_736_921_722.0, 20_254.0, 20_254.0, 20_254.0, 22_522.0],
                [-356_840_880.0, -4_124.0, -4_124.0, -4_124.0, 77_520.0],
            ];
            let rows = |kept: &[usize]| -> Vec<Vec<ValueRef<'static>>> {
                kept.iter()
                    .map(|&row| stored[row].map(float).to_vec())
                    .collect()
            };
            // The flag over an all-zero bitmap deletes nothing.
            let mut flagged = original.clone();
            flagged[1040] = 0x80;
            let table = sas_source::parse(&flagged, &source).unwrap();
            assert_eq!(cells(&table), rows(&[0, 1, 2, 3]));
            // Counting one deleted record and marking the second removes it.
            let mut deleted = flagged;
            deleted[4668] = 1;
            deleted[3764] = 0b0100_0000;
            let table = sas_source::parse(&deleted, &source).unwrap();
            assert_eq!(cells(&table), rows(&[0, 2, 3]));
        }
        #[test]
        fn counted_deletions_without_marks_are_refused_with_a_rewrite_hint() {
            // A deleted count of one beside the haven fixture's four stored
            // records, with no page flagged, leaves the counted deletion
            // unmarked, so a deleted record could only be read as live.
            let source = declaration("datetime.sas7bdat");
            let mut data = bytes(&source.path);
            assert_eq!(&data[4664..4672], &[4, 0, 0, 0, 0, 0, 0, 0]);
            data[4668] = 1;
            let error = sas_source::parse(&data, &source).unwrap_err();
            assert!(matches!(error, Error::Invalid("deleted_records")));
            let error = yamaa_adapters::specification_run::Error::SasSource(error);
            assert_eq!(
                crate::support::rows(
                    &yamaa_adapters::specification_diagnostics::findings(&error, Some(&source))
                        .unwrap()
                        .unwrap()
                ),
                [yamaa_adapters::json!({
                    "phase": "ingest",
                    "condition": "source_sas_invalid",
                    "rule": "SAS-7",
                    "spec_paths": ["input.SRC.path"],
                    "context": {
                        "dataset": "SRC",
                        "path": "datetime.sas7bdat",
                        "cause": "deleted_records",
                        "hint": "rewrite the SAS dataset first, for example with PROC \
                                 COPY or a DATA step, to remove the records deleted in place"
                    }
                })]
            );
        }
        #[test]
        fn deleted_compressed_record_is_skipped() {
            // Independent pandas RLE layout: the row-size subheader at 130592
            // counts 10 records, none deleted, and the first compressed-row
            // pointer's compression byte is at 66840. SAS marks a deleted
            // compressed row with code 0x05 and counts it as deleted.
            let expected =
                yamaa_adapters::csv_source::parse(&bytes("compressed-truth.csv")).unwrap();
            let source = declaration("rle.sas7bdat");
            let mut data = bytes(&source.path);
            assert_eq!(&data[130616..130624], &[10, 0, 0, 0, 0, 0, 0, 0]);
            assert_eq!(data[66840], 0x04);
            data[66840] = 0x05;
            data[130620] = 1;
            let table = sas_source::parse(&data, &source).unwrap();
            assert_compressed_truth(&table, &expected.names, &expected.records[1..]);
        }
        #[test]
        fn deleted_row_bitmaps_skip_records_in_every_word_size_and_byte_order() {
            use crate::support::sas7bdat::{Cell, Dataset, Kind, Layout, Page, MIX};
            for layout in [Layout::LE32, Layout::LE64, Layout::BE32, Layout::BE64] {
                let mut dataset = Dataset::new(layout, &[("X", Kind::Num, 8), ("T", Kind::Chr, 8)]);
                dataset.total = 5;
                dataset.deleted = 2;
                dataset.mix_rows = 5;
                let rows = (0..5)
                    .map(|row| {
                        let label = format!("r{row}");
                        dataset.row(&[Cell::Num(row as f64 + 0.5), Cell::Text(label.as_bytes())])
                    })
                    .collect();
                let mut page = Page::new(MIX, dataset.metadata(), rows);
                page.deleted = Some(vec![false, true, false, true, false]);
                let table = read_built(&dataset.file(&[page])).unwrap();
                assert_eq!(
                    cells(&table),
                    [
                        [float(0.5), ValueRef::Str("r0")],
                        [float(2.5), ValueRef::Str("r2")],
                        [float(4.5), ValueRef::Str("r4")]
                    ],
                    "{layout:?}"
                );
            }
        }
        #[test]
        fn deleted_records_after_the_last_live_record_are_found_on_later_pages() {
            // The last two records of the first data page and the whole last
            // page are deleted, after the last live record.
            use crate::support::sas7bdat::{Cell, Dataset, Kind, Layout, Page, DATA, MIX};
            let mut dataset = Dataset::new(Layout::LE64, &[("X", Kind::Num, 8)]);
            dataset.total = 9;
            dataset.deleted = 5;
            dataset.mix_rows = 3;
            let row = |value| dataset.row(&[Cell::Num(value)]);
            let mut mix = Page::new(MIX, dataset.metadata(), vec![row(0.0), row(1.0), row(2.0)]);
            mix.deleted = Some(vec![false, false, true]);
            let rows = vec![row(3.0), row(4.0), row(5.0), row(6.0)];
            let mut data = Page::new(DATA, vec![], rows);
            data.deleted = Some(vec![false, false, true, true]);
            let mut gone = Page::new(DATA, vec![], vec![row(7.0), row(8.0)]);
            gone.deleted = Some(vec![true, true]);
            let pages = [mix, data, gone];
            let table = read_built(&dataset.file(&pages)).unwrap();
            assert_eq!(
                cells(&table),
                [[float(0.0)], [float(1.0)], [float(3.0)], [float(4.0)]]
            );
            // Without the last page two counted deletions are never found.
            assert!(matches!(
                read_built(&dataset.file(&pages[..2])),
                Err(Error::Invalid("deleted_records"))
            ));
        }
        #[test]
        fn marks_on_pages_after_the_counts_are_met_are_not_read() {
            // The check compares counts over the pages the reader reaches. It
            // finds the live record and the one counted deletion on the first
            // page, so it stops there and never reads the marks that a later
            // page carries beyond the count.
            use crate::support::sas7bdat::{Cell, Dataset, Kind, Layout, Page, DATA, MIX};
            let mut dataset = Dataset::new(Layout::LE64, &[("X", Kind::Num, 8)]);
            dataset.total = 2;
            dataset.deleted = 1;
            dataset.mix_rows = 2;
            let row = |value| dataset.row(&[Cell::Num(value)]);
            let mut first = Page::new(MIX, dataset.metadata(), vec![row(1.0), row(2.0)]);
            first.deleted = Some(vec![false, true]);
            let mut later = Page::new(DATA, vec![], vec![row(3.0), row(4.0)]);
            later.deleted = Some(vec![true, true]);
            let table = read_built(&dataset.file(&[first, later])).unwrap();
            assert_eq!(cells(&table), [[float(1.0)]]);
        }
        #[test]
        fn deleted_compressed_rows_in_a_64_bit_file_are_skipped_and_counted() {
            use crate::support::sas7bdat::{rle, Cell, Dataset, Kind, Layout, Page, Pointer, META};
            let mut dataset = Dataset::new(Layout::LE64, &[("X", Kind::Num, 8)]);
            dataset.compression = Some(*b"SASYZCRL");
            dataset.total = 3;
            let file = |dataset: &Dataset| {
                let mut pointers = dataset.metadata();
                pointers.push(Pointer::rle_row(&dataset.row(&[Cell::Num(1.0)])));
                pointers.push(Pointer::coded(rle(&dataset.row(&[Cell::Num(2.0)])), 0x05));
                pointers.push(Pointer::raw_row(dataset.row(&[Cell::Num(3.0)])));
                dataset.file(&[Page::new(META, pointers, vec![])])
            };
            dataset.deleted = 1;
            let table = read_built(&file(&dataset)).unwrap();
            assert_eq!(cells(&table), [[float(1.0)], [float(3.0)]]);
            // A record marked deleted that the row-size subheader does not
            // count cannot be reconciled with it.
            dataset.deleted = 0;
            assert!(matches!(
                read_built(&file(&dataset)),
                Err(Error::Invalid("deleted_records"))
            ));
        }
        #[test]
        fn deleted_record_counts_that_disagree_with_their_marks_are_refused() {
            use crate::support::sas7bdat::{Cell, Dataset, Kind, Layout, Page, MIX};
            let file = |total: u64, deleted: u64, rows: usize, mix: u64, marks: &[bool]| {
                let mut dataset = Dataset::new(Layout::LE64, &[("X", Kind::Num, 8)]);
                dataset.total = total;
                dataset.deleted = deleted;
                dataset.mix_rows = mix;
                let rows = (0..rows).map(|row| dataset.row(&[Cell::Num(row as f64)]));
                let mut page = Page::new(MIX, dataset.metadata(), rows.collect());
                page.deleted = Some(marks.to_vec());
                (dataset, page)
            };
            let refused = |(dataset, page): (Dataset, Page)| {
                let result = read_built(&dataset.file(&[page]));
                assert!(matches!(result, Err(Error::Invalid("deleted_records"))));
            };
            let marks = [false, true, false, false];
            // More deleted records than stored ones.
            refused(file(4, 5, 4, 4, &marks));
            // More marks than counted deletions, and fewer.
            refused(file(4, 1, 4, 4, &[false, true, true, false]));
            refused(file(4, 2, 4, 4, &marks));
            // A free-space field that places the bitmap past the page end,
            // including one that would wrap a 64-bit sum.
            for free_space in [4096, u64::MAX - 7] {
                let (dataset, mut page) = file(4, 1, 4, 4, &marks);
                page.free_space = Some(free_space);
                refused((dataset, page));
            }
            // A mix page that may hold five records while the dataset stores
            // two: its bitmap covers two, and the unmarked deletion is not
            // looked for beyond them.
            refused(file(2, 1, 2, 5, &[false, false, true]));
        }
        #[test]
        fn the_row_limit_admits_live_records_not_stored_ones() {
            // The row-size subheader counts stored records, deleted ones
            // included, but only the live ones become rows, so SAS-6 admits
            // those. The files hold no records: a count that passes admission
            // goes on to fail its unmet deletions, and one that does not is a
            // table-rows limit that names the live count.
            use crate::support::sas7bdat::{Dataset, Kind, Layout, Page, META};
            let file = |total: u64, deleted: u64| {
                let mut dataset = Dataset::new(Layout::LE64, &[("X", Kind::Num, 8)]);
                dataset.total = total;
                dataset.deleted = deleted;
                dataset.file(&[Page::new(META, dataset.metadata(), vec![])])
            };
            // 10,000,100 stored records leave 9,999,900 live ones, under the
            // limit of 10,000,000 rows.
            assert!(matches!(
                read_built(&file(10_000_100, 200)),
                Err(Error::Invalid("deleted_records"))
            ));
            // 10,000,050 live records are over it.
            assert!(matches!(
                read_built(&file(10_000_100, 50)),
                Err(Error::Limit {
                    resource: "table_rows",
                    actual: Some(10_000_050),
                    ..
                })
            ));
        }
        #[test]
        fn stored_record_counts_beyond_32_bits_are_refused() {
            // The reader counts stored and deleted records in 32 bits. The
            // counts below leave five live records, so they pass admission,
            // but truncated they would read as a dataset with no deletions.
            use crate::support::sas7bdat::{Cell, Dataset, Kind, Layout, Page, MIX};
            let file = |total: u64, deleted: u64| {
                let mut dataset = Dataset::new(Layout::LE64, &[("X", Kind::Num, 8)]);
                dataset.total = total;
                dataset.deleted = deleted;
                dataset.mix_rows = 5;
                let rows = (0..5).map(|row| dataset.row(&[Cell::Num(f64::from(row))]));
                dataset.file(&[Page::new(MIX, dataset.metadata(), rows.collect())])
            };
            assert_eq!(read_built(&file(5, 0)).unwrap().row_count(), 5);
            assert!(matches!(
                read_built(&file((1 << 40) + 5, 1 << 40)),
                Err(Error::Invalid("malformed"))
            ));
        }
        /// A compressed dataset whose rows are 4,096 bytes: the field `X`, then
        /// a gap. It counts `total` live records and stores `rows`, compressed
        /// with RDC or RLE, in place on its first page, or moved to its second
        /// page and read through references.
        fn padded_rows(
            layout: crate::support::sas7bdat::Layout,
            rdc: bool,
            moved: bool,
            total: u64,
            rows: &[Vec<u8>],
        ) -> Vec<u8> {
            use crate::support::sas7bdat::{
                rdc as rdc_bytes, rle, Dataset, Kind, Page, Pointer, META,
            };
            let mut dataset = Dataset::new(layout, &[("X", Kind::Num, 8)]);
            dataset.compression = Some(if rdc { *b"SASYZCR2" } else { *b"SASYZCRL" });
            dataset.row_length = 4096;
            dataset.page_len = 32768;
            dataset.total = total;
            let compress = |row: &[u8]| if rdc { rdc_bytes(row) } else { rle(row) };
            let mut pointers = dataset.metadata();
            let mut elsewhere = Vec::new();
            for (index, row) in rows.iter().enumerate() {
                if moved {
                    pointers.push(Pointer::reference(1, index as u64));
                    elsewhere.push(Pointer::coded(compress(row), 0x06));
                } else {
                    pointers.push(Pointer::coded(compress(row), 4));
                }
            }
            let mut pages = vec![Page::new(META, pointers, vec![])];
            if moved {
                pages.push(Page::new(META, elsewhere, vec![]));
            }
            dataset.file(&pages)
        }
        #[test]
        fn decompressed_records_are_admitted_as_source_bytes_before_any_is_decompressed() {
            // A compressed record decompresses to the row length, so the live
            // records times the row length are the bytes that decompression
            // writes. SAS-6 admits them under the source byte limit before any
            // record is decompressed. With 4,096-byte rows, 2,621,440 records
            // are exactly the 10 GiB limit. The record stored here decompresses
            // to 16 bytes, not 4,096: a file admitted at the limit fails as
            // malformed when that record is decompressed, and a file that
            // counts one record more is refused before it is.
            use crate::support::sas7bdat::Layout;
            let limit = yamaa_core::limits::Limits::SOURCE_BYTES;
            let edge = (limit / 4096) as u64;
            for layout in [Layout::LE32, Layout::LE64, Layout::BE32, Layout::BE64] {
                let row = |value: f64| {
                    let mut row = layout.double(value).to_vec();
                    row.resize(4096, 0);
                    row
                };
                let short = row(9.0)[..16].to_vec();
                for rdc in [false, true] {
                    for moved in [false, true] {
                        let case = format!("{layout:?} rdc {rdc} moved {moved}");
                        let read = |total, rows: &[Vec<u8>]| {
                            read_built(&padded_rows(layout, rdc, moved, total, rows))
                        };
                        assert!(
                            matches!(
                                read(edge, std::slice::from_ref(&short)),
                                Err(Error::Invalid("malformed"))
                            ),
                            "{case}"
                        );
                        match read(edge + 1, std::slice::from_ref(&short)) {
                            Err(Error::Limit {
                                resource: "source_bytes",
                                limit: refused,
                                actual: Some(actual),
                            }) => assert_eq!((refused, actual), (limit, limit + 4096), "{case}"),
                            other => panic!("{case}: {:?}", other.map(|table| table.row_count())),
                        }
                        // Within the limit the records read. Decompression stops
                        // at the live records, so the short record after the
                        // two that the file counts is not decompressed.
                        let rows = [row(1.5), row(-2.0), short.clone()];
                        let table =
                            read(2, &rows).unwrap_or_else(|error| panic!("{case}: {error:?}"));
                        assert_eq!(cells(&table), [[float(1.5)], [float(-2.0)]], "{case}");
                    }
                }
            }
        }
        #[test]
        fn a_small_file_cannot_decompress_to_more_than_the_source_byte_limit() {
            // A two-byte RLE command writes up to 4,112 zero bytes, so 255 of
            // them, 510 stored bytes, make a record of 1,048,560 bytes. A file
            // of one 6 MiB page then holds 10,241 such records, every record it
            // counts, and they decompress to 10,738,302,960 bytes, more than
            // the 10 GiB limit. The reader refuses the file at its first record
            // instead of writing them.
            use crate::support::sas7bdat::{Dataset, Kind, Layout, Page, Pointer, META};
            const ROW: usize = 255 * 4112;
            let limit = yamaa_core::limits::Limits::SOURCE_BYTES;
            let records = limit / ROW + 1;
            let mut dataset = Dataset::new(Layout::LE32, &[("X", Kind::Num, 8)]);
            dataset.compression = Some(*b"SASYZCRL");
            dataset.row_length = ROW as u64;
            dataset.page_len = 6 << 20;
            dataset.total = records as u64;
            // Command 7, with length nibble 15 and count byte 0xFF, inserts
            // 17 + 0xFF + 15 * 256 = 4,112 zero bytes.
            let record = [0x7F, 0xFF].repeat(255);
            let mut pointers = dataset.metadata();
            pointers.extend((0..records).map(|_| Pointer::coded(record.clone(), 4)));
            let bytes = dataset.file(&[Page::new(META, pointers, vec![])]);
            match read_built(&bytes) {
                Err(Error::Limit {
                    resource: "source_bytes",
                    limit: refused,
                    actual: Some(actual),
                }) => assert_eq!((refused, actual), (limit, records * ROW)),
                other => panic!("{:?}", other.map(|table| table.row_count())),
            }
        }
        /// A compressed dataset whose third record moved to subheader 1 of
        /// page 1, the record stored there by `moved`.
        fn moved_file(
            layout: crate::support::sas7bdat::Layout,
            deleted: u64,
            reference: crate::support::sas7bdat::Pointer,
            moved: impl Fn(&crate::support::sas7bdat::Dataset) -> crate::support::sas7bdat::Page,
        ) -> Vec<u8> {
            use crate::support::sas7bdat::{Cell, Dataset, Kind, Page, Pointer, META};
            let mut dataset = Dataset::new(layout, &[("X", Kind::Num, 8), ("T", Kind::Chr, 8)]);
            dataset.compression = Some(*b"SASYZCRL");
            dataset.total = 4;
            dataset.deleted = deleted;
            let row =
                |value: f64, label: &[u8]| dataset.row(&[Cell::Num(value), Cell::Text(label)]);
            let mut first = dataset.metadata();
            first.push(Pointer::rle_row(&row(1.0, b"one")));
            first.push(Pointer::rle_row(&row(2.0, b"two")));
            first.push(reference);
            first.push(Pointer::rle_row(&row(4.0, b"four")));
            dataset.file(&[Page::new(META, first, vec![]), moved(&dataset)])
        }
        #[test]
        fn moved_records_are_read_at_their_reference() {
            // SAS can move an updated compressed record that no longer fits
            // its slot to another page. The slot keeps a reference (code
            // 0x03) whose offset and length fields hold the one-based page
            // and subheader indices of the moved record. That record carries
            // code 0x02 stored uncompressed or 0x06 compressed, and is read
            // only through its reference.
            use crate::support::sas7bdat::{rle, Cell, Layout, Page, Pointer, META};
            for layout in [Layout::LE64, Layout::BE32] {
                for code in [0x02, 0x06] {
                    let bytes = moved_file(layout, 0, Pointer::reference(1, 1), |dataset| {
                        let row = dataset.row(&[Cell::Num(3.0), Cell::Text(b"three")]);
                        let stored = if code == 0x02 { row } else { rle(&row) };
                        Page::new(
                            META,
                            vec![Pointer::truncated(), Pointer::coded(stored, code)],
                            vec![],
                        )
                    });
                    let table = read_built(&bytes).unwrap();
                    assert_eq!(
                        cells(&table),
                        [
                            [float(1.0), ValueRef::Str("one")],
                            [float(2.0), ValueRef::Str("two")],
                            [float(3.0), ValueRef::Str("three")],
                            [float(4.0), ValueRef::Str("four")]
                        ],
                        "{layout:?} {code:#04x}"
                    );
                }
            }
        }
        /// A compressed dataset of four records whose third moved. The moved
        /// record, stored with `code`, comes before the reference to it: on
        /// the page before the reference's page, or earlier on the same page.
        fn moved_before_its_reference(
            layout: crate::support::sas7bdat::Layout,
            rdc: bool,
            code: u8,
            same_page: bool,
        ) -> Vec<u8> {
            use crate::support::sas7bdat::{
                rdc as rdc_bytes, rle, Cell, Dataset, Kind, Page, Pointer, META,
            };
            let mut dataset = Dataset::new(layout, &[("X", Kind::Num, 8), ("T", Kind::Chr, 8)]);
            dataset.compression = Some(if rdc { *b"SASYZCR2" } else { *b"SASYZCRL" });
            dataset.total = 4;
            let compress = |row: &[u8]| if rdc { rdc_bytes(row) } else { rle(row) };
            let row =
                |value: f64, label: &[u8]| dataset.row(&[Cell::Num(value), Cell::Text(label)]);
            let three = row(3.0, b"three");
            let moved = Pointer::coded(
                if code == 0x02 {
                    three
                } else {
                    compress(&three)
                },
                code,
            );
            let mut first = dataset.metadata();
            first.push(Pointer::coded(compress(&row(1.0, b"one")), 4));
            first.push(Pointer::coded(compress(&row(2.0, b"two")), 4));
            let four = Pointer::coded(compress(&row(4.0, b"four")), 4);
            let pages = if same_page {
                let index = first.len() as u64;
                first.extend([moved, Pointer::reference(0, index), four]);
                vec![Page::new(META, first, vec![])]
            } else {
                vec![
                    Page::new(META, first, vec![]),
                    Page::new(META, vec![Pointer::truncated(), moved], vec![]),
                    Page::new(META, vec![Pointer::reference(1, 1), four], vec![]),
                ]
            };
            dataset.file(&pages)
        }
        #[test]
        fn a_moved_record_stored_before_its_reference_is_skipped_and_read_at_the_reference() {
            // The second pass meets the moved record first, where it skips
            // it, and reads it when it reaches the reference. The records
            // come out in the order of the references, with the moved one
            // third. Codes 0x02 and 0x06 are stored uncompressed and
            // compressed, and the compressed record is RLE or RDC.
            use crate::support::sas7bdat::Layout;
            for layout in [Layout::LE64, Layout::BE32] {
                for (rdc, code) in [(false, 0x02), (false, 0x06), (true, 0x02), (true, 0x06)] {
                    for same_page in [false, true] {
                        let bytes = moved_before_its_reference(layout, rdc, code, same_page);
                        assert_eq!(
                            cells(&read_built(&bytes).unwrap()),
                            [
                                [float(1.0), ValueRef::Str("one")],
                                [float(2.0), ValueRef::Str("two")],
                                [float(3.0), ValueRef::Str("three")],
                                [float(4.0), ValueRef::Str("four")]
                            ],
                            "{layout:?} rdc {rdc} {code:#04x} same page {same_page}"
                        );
                    }
                }
            }
        }
        #[test]
        fn moved_records_deleted_later_count_as_deleted() {
            // Deleting a moved record truncates its reference and gives the
            // record code 0x09 (uncompressed) or 0x0d (compressed). No
            // reference reaches it, and the dataset counts it as deleted.
            use crate::support::sas7bdat::{rle, Cell, Layout, Page, Pointer, META};
            for code in [0x09, 0x0d] {
                let file = |deleted| {
                    moved_file(Layout::LE64, deleted, Pointer::truncated(), |dataset| {
                        let row = dataset.row(&[Cell::Num(3.0), Cell::Text(b"three")]);
                        let stored = if code == 0x09 { row } else { rle(&row) };
                        Page::new(META, vec![Pointer::coded(stored, code)], vec![])
                    })
                };
                let table = read_built(&file(1)).unwrap();
                assert_eq!(
                    cells(&table),
                    [
                        [float(1.0), ValueRef::Str("one")],
                        [float(2.0), ValueRef::Str("two")],
                        [float(4.0), ValueRef::Str("four")]
                    ],
                    "{code:#04x}"
                );
                assert!(matches!(
                    read_built(&file(0)),
                    Err(Error::Invalid("deleted_records"))
                ));
            }
        }
        #[test]
        fn references_that_do_not_reach_a_moved_record_are_refused() {
            use crate::support::sas7bdat::{Cell, Layout, Page, Pointer, DATA, META};
            let moved = |dataset: &crate::support::sas7bdat::Dataset| {
                let row = dataset.row(&[Cell::Num(3.0), Cell::Text(b"three")]);
                Page::new(META, vec![Pointer::coded(row, 0x02)], vec![])
            };
            // A page beyond the file, a page index of zero (stored as offset
            // zero), and a subheader index beyond the page's pointers.
            let zero = Pointer::Raw {
                offset: 0,
                length: 1,
                compression: 3,
                data: true,
            };
            for reference in [Pointer::reference(2, 0), zero, Pointer::reference(1, 1)] {
                let result = read_built(&moved_file(Layout::LE64, 0, reference, moved));
                assert!(matches!(result, Err(Error::Invalid("malformed"))));
            }
            // A data page holds no subheaders to reference.
            let bytes = moved_file(Layout::LE64, 0, Pointer::reference(1, 0), |dataset| {
                let row = dataset.row(&[Cell::Num(3.0), Cell::Text(b"three")]);
                Page::new(DATA, vec![], vec![row])
            });
            assert!(matches!(
                read_built(&bytes),
                Err(Error::Invalid("malformed"))
            ));
            // A moved record stored uncompressed must be marked as row data.
            let bytes = moved_file(Layout::LE64, 0, Pointer::reference(1, 0), |dataset| {
                let row = dataset.row(&[Cell::Num(3.0), Cell::Text(b"three")]);
                let pointer = Pointer::Stored {
                    bytes: row,
                    compression: 0x02,
                    data: false,
                };
                Page::new(META, vec![pointer], vec![])
            });
            assert!(matches!(
                read_built(&bytes),
                Err(Error::Invalid("malformed"))
            ));
            // A record that was not moved there has another compression code.
            let bytes = moved_file(Layout::LE64, 0, Pointer::reference(1, 0), |dataset| {
                let row = dataset.row(&[Cell::Num(3.0), Cell::Text(b"three")]);
                Page::new(META, vec![Pointer::rle_row(&row)], vec![])
            });
            assert!(matches!(
                read_built(&bytes),
                Err(Error::Invalid("compression"))
            ));
        }
        #[test]
        fn a_reference_with_a_zero_page_or_subheader_index_is_refused_not_skipped() {
            // A reference holds one-based page and subheader indices, so a
            // zero index names no record. The dataset below stores every
            // record it declares in place, so skipping such a reference would
            // read the file as valid.
            use crate::support::sas7bdat::{Cell, Dataset, Kind, Layout, Page, Pointer, META};
            let file = |reference: Option<Pointer>| {
                let mut dataset = Dataset::new(Layout::LE64, &[("X", Kind::Num, 8)]);
                dataset.compression = Some(*b"SASYZCRL");
                dataset.total = 2;
                let mut pointers = dataset.metadata();
                pointers.push(Pointer::rle_row(&dataset.row(&[Cell::Num(1.0)])));
                pointers.extend(reference);
                pointers.push(Pointer::rle_row(&dataset.row(&[Cell::Num(2.0)])));
                dataset.file(&[Page::new(META, pointers, vec![])])
            };
            assert_eq!(
                cells(&read_built(&file(None)).unwrap()),
                [[float(1.0)], [float(2.0)]]
            );
            for (offset, length) in [(1, 0), (0, 0), (0, 1)] {
                let reference = Pointer::Raw {
                    offset,
                    length,
                    compression: 3,
                    data: true,
                };
                assert!(
                    matches!(
                        read_built(&file(Some(reference))),
                        Err(Error::Invalid("malformed"))
                    ),
                    "{offset} {length}"
                );
            }
        }
        /// A compressed dataset of `X` and a 1,000-byte text column whose every
        /// record moved. Page 0 holds one reference for each of `reached`,
        /// naming a page and a subheader index. Pages 1 to `moved_pages` hold
        /// `per_page` moved records each, of 1,071 stored bytes.
        fn referenced_records(
            layout: crate::support::sas7bdat::Layout,
            page_len: usize,
            moved_pages: u64,
            per_page: u64,
            reached: &[(u64, u64)],
        ) -> Vec<u8> {
            use crate::support::sas7bdat::{rle, Cell, Dataset, Kind, Page, Pointer, META};
            let mut dataset = Dataset::new(layout, &[("X", Kind::Num, 8), ("T", Kind::Chr, 1000)]);
            dataset.compression = Some(*b"SASYZCRL");
            dataset.page_len = page_len;
            dataset.total = reached.len() as u64;
            let mut pointers = dataset.metadata();
            pointers.extend(
                reached
                    .iter()
                    .map(|&(page, index)| Pointer::reference(page, index)),
            );
            let mut pages = vec![Page::new(META, pointers, vec![])];
            for page in 1..=moved_pages {
                let records = (0..per_page)
                    .map(|index| {
                        let label = format!("page{page}index{index}");
                        let number = (page * 100 + index) as f64;
                        let row = dataset.row(&[Cell::Num(number), Cell::Text(label.as_bytes())]);
                        Pointer::coded(rle(&row), 0x06)
                    })
                    .collect();
                pages.push(Page::new(META, records, vec![]));
            }
            dataset.file(&pages)
        }
        #[test]
        fn references_that_reach_each_moved_record_once_read_across_pages() {
            // Five pages hold three moved records each. The references visit
            // the pages in turn, so no two in a row reach the same page, and
            // together reach every record once.
            use crate::support::sas7bdat::Layout;
            let reached: Vec<(u64, u64)> = (0..15).map(|at| (1 + at % 5, at / 5)).collect();
            let labels: Vec<String> = reached
                .iter()
                .map(|&(page, index)| format!("page{page}index{index}"))
                .collect();
            let expected: Vec<Vec<ValueRef<'_>>> = reached
                .iter()
                .zip(&labels)
                .map(|(&(page, index), label)| {
                    vec![float((page * 100 + index) as f64), ValueRef::Str(label)]
                })
                .collect();
            for layout in [Layout::LE32, Layout::LE64, Layout::BE32, Layout::BE64] {
                let bytes = referenced_records(layout, 4096, 5, 3, &reached);
                let table = read_built(&bytes).unwrap();
                assert_eq!(cells(&table), expected, "{layout:?}");
            }
        }
        #[test]
        fn references_bouncing_between_pages_cannot_read_more_than_the_file_holds() {
            // Each of 60,000 references reaches a valid moved record of 1,071
            // stored bytes, alternating between the records on two 1 MiB pages.
            // Together they would read about 64 MB from a file of 3 MiB. A
            // reference reads only the record it reaches, and the stored bytes
            // that references read may not exceed the bytes of the pages.
            use crate::support::sas7bdat::Layout;
            let reached: Vec<(u64, u64)> = (0..60_000).map(|at| (1 + at % 2, 0)).collect();
            let bytes = referenced_records(Layout::LE32, 1 << 20, 2, 1, &reached);
            assert_eq!(bytes.len(), 1024 + 3 * (1 << 20));
            assert!(matches!(
                read_built(&bytes),
                Err(Error::Invalid("malformed"))
            ));
        }
        #[test]
        fn reject_multiple_members_in_both_transport_versions() {
            for name in ["multi-v5.xpt", "multi-v8.xpt"] {
                assert!(matches!(
                    sas_source::parse(&bytes(name), &declaration(name)),
                    Err(Error::Invalid("members"))
                ));
            }
        }
        #[test]
        fn member_header_shaped_character_data_is_an_ordinary_value() {
            let source = declaration("member-text.xpt");
            let table = sas_source::parse(&bytes(&source.path), &source).unwrap();
            assert_eq!(table.row_count(), 1);
            assert_eq!(
        table.cell(0, 0).unwrap(),
        ValueRef::Str(
            "HEADER RECORD*******MEMBER  HEADER RECORD!!!!!!!000000000000000000000000000000"
        )
    );
        }
        #[test]
        fn ordinary_float_keeps_readstat_conversion_while_exact_narrowing_is_guarded() {
            for (file, expected) in [
                ("decimal.xpt", 0.1),
                ("precision.xpt", 9_007_199_254_740_992.0),
            ] {
                let source = declaration(file);
                let table = sas_source::parse(&bytes(file), &source).unwrap();
                assert_eq!(
                    table.cell(0, 0).unwrap(),
                    ValueRef::Float(yamaa_core::value::FiniteFloat::new(expected).unwrap())
                );
            }
        }
        #[test]
        fn narrowing_never_truncates_or_accepts_an_ibm_precision_loss() {
            for name in ["fractional.xpt", "precision.xpt"] {
                for kind in [ColumnType::Int, ColumnType::Date, ColumnType::DateTime] {
                    let mut source = declaration(name);
                    source.types = vec![("NUM".into(), kind)];
                    assert!(matches!(
                        sas_source::parse(&bytes(name), &source),
                        Err(Error::Typing(
                            yamaa_core::typed_csv::Error::FieldParse { .. }
                        ))
                    ));
                }
            }
        }
        #[test]
        fn malformed_truncated_encoding_and_unknown_fields_fail_before_execution() {
            let source = declaration("v5.xpt");
            let payload = bytes("v5.xpt");
            for length in [0, 1, 79, 640, payload.len() - 1] {
                assert!(sas_source::parse(&payload[..length], &source).is_err());
            }
            let mut source = source;
            source.encoding = Some("utf-8".into());
            assert!(matches!(
                sas_source::parse(&payload, &source),
                Err(Error::Invalid("encoding"))
            ));
            source.encoding = Some("windows-1252".into());
            source.types = vec![("ABSENT".into(), ColumnType::Int)];
            assert!(matches!(
                sas_source::parse(&payload, &source),
                Err(Error::Typing(
                    yamaa_core::typed_csv::Error::UnknownField { .. }
                ))
            ));
            let sas = bytes("datetime.sas7bdat");
            for length in [0, 31, 1024, sas.len() - 1] {
                assert!(matches!(
                    sas_source::parse(&sas[..length], &declaration("datetime.sas7bdat"),),
                    Err(Error::Invalid(_))
                ));
            }
        }
    }
    mod source_diagnostics {
        //! Borrowed codec projections compare independently captured complete diagnostics.
        //!
        //! Seam, for every test here: codec errors built in memory, nested or
        //! wide error trees, the identity of borrowed values and the order of
        //! opaque barriers, which a case reaches only through a whole run.
        use yamaa_adapters::json;
        use yamaa_adapters::{
            csv_source::{self, TextTableError},
            parquet_source as parquet, source_diagnostics, specification_diagnostics as projection,
            specification_run::Error,
            typed_csv,
        };
        use yamaa_core::json::Value;
        use yamaa_core::{
            diagnostic::{ParquetFinding, SourceFinding},
            specification::{SourceDeclaration, SourceProfile},
            value::ColumnType,
        };

        fn source(name: &str, path: &str) -> SourceDeclaration {
            SourceDeclaration {
                name: name.into(),
                path: path.into(),
                types: vec![],
                profile: SourceProfile::from_path(path).unwrap(),
                encoding: None,
                byte_order_mark: false,
                empty_string_present: false,
                ordinal: None,
            }
        }

        fn bytes(hex: &str) -> Vec<u8> {
            hex.as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect()
        }
        fn assert_complete(error: &Error, source: &SourceDeclaration, expected: &Value) {
            let wanted = expected.as_array().unwrap();
            assert_eq!(
                crate::support::rows(&projection::findings(error, Some(source)).unwrap().unwrap()),
                *wanted
            );
            assert_eq!(
                crate::support::findings(projection::failure(error, Some(source))),
                *expected
            );
        }

        #[test]
        fn all_independent_csv_profile_reports_preserve_original_projection() {
            let declaration = source("SRC", "source.csv");
            let mut count = 0;
            for row in yamaa_adapters::json_wire::decode_array(include_str!(
                "fixtures/csv_profile_diagnostics.json"
            ))
            .unwrap()
            .iter()
            {
                let expected: Value = row["expected"].clone();
                let source_bytes: Vec<u8> = if let Some(text) = row["source_bytes"].as_str() {
                    text.as_bytes().to_vec()
                } else {
                    bytes(row["source_bytes"]["hex"].as_str().unwrap())
                };
                let error = Error::Source(TextTableError::Csv(
                    csv_source::parse(&source_bytes).unwrap_err(),
                ));
                assert_complete(&error, &declaration, &expected["diagnostics"]);
                count += 1;
            }
            assert_eq!(count, 13);
        }

        #[test]
        fn all_independent_typing_reports_keep_original_first_bad_field_and_value() {
            let declaration = source("SRC", "source.csv");
            let mut count = 0;
            for row in
                yamaa_adapters::json_wire::decode_array(include_str!("fixtures/source_typing.json"))
                    .unwrap()
                    .iter()
            {
                let expected: Value = row["expected"].clone();
                let types = match row["case"].as_str().unwrap() {
                    "unknown-field" => vec![
                        ("ID".into(), ColumnType::Int),
                        ("ABSENT".into(), ColumnType::Int),
                    ],
                    "unknown-before-conversion" => vec![
                        ("V".into(), ColumnType::Int),
                        ("ABSENT".into(), ColumnType::Date),
                    ],
                    "stored-order" => {
                        vec![("B".into(), ColumnType::Int), ("A".into(), ColumnType::Int)]
                    }
                    "invalid-temporal" => vec![("V".into(), ColumnType::Date)],
                    _ => panic!("unknown independent fixture"),
                };
                let source_bytes: Vec<u8> = if let Some(text) = row["source_bytes"].as_str() {
                    text.as_bytes().to_vec()
                } else {
                    bytes(row["source_bytes"]["hex"].as_str().unwrap())
                };
                let error =
                    Error::TypedSource(typed_csv::parse(&source_bytes, &types).unwrap_err());
                assert_complete(&error, &declaration, &expected["diagnostics"]);
                count += 1;
            }
            assert_eq!(count, 4);
        }

        #[test]
        #[cfg(feature = "parquet")]
        fn independently_authored_parquet_schemas_keep_exact_spelling_and_raw_value() {
            let declaration = source("SRC", "held.parquet");
            for (fixture, condition, rule, extra) in [
                ("", "source_parquet_invalid", "PARQUET-5", json!({})),
                (
                    "mixed-empty",
                    "source_field_name_empty",
                    "PARQUET-6",
                    json!({"field":1}),
                ),
                (
                    "mixed-duplicate",
                    "source_field_name_duplicate",
                    "PARQUET-6",
                    json!({"field":"I"}),
                ),
                (
                    "mixed-struct",
                    "source_field_type_unsupported",
                    "PARQUET-7",
                    json!({"field":"S","stored_type":"struct<left: int64, right: string>"}),
                ),
                (
                    "utf8-time",
                    "source_field_value_invalid",
                    "PARQUET-9",
                    json!({"field":"OTHER","record":1,"value":1}),
                ),
                (
                    "bad-date",
                    "source_field_value_invalid",
                    "PARQUET-9",
                    json!({"field":"D","record":1,"value":-719163}),
                ),
            ] {
                let bytes = if fixture.is_empty() {
                    Vec::new()
                } else {
                    crate::support::parquet_files::file(fixture)
                };
                let error = Error::ParquetSource(parquet::parse(&bytes).unwrap_err());
                let mut context = json!({"dataset":"SRC","path":"held.parquet"});
                context
                    .as_object_mut()
                    .unwrap()
                    .extend(extra.as_object().unwrap().clone());
                assert_complete(
                    &error,
                    &declaration,
                    &json!([{"phase":"ingest","condition":condition,
            "rule":rule,"spec_paths":["input.SRC.path"],"context":context}]),
                );
            }
        }

        #[test]
        fn nested_ordered_report_keeps_complete_borrowed_codec_facts() {
            let error = Error::Sources(vec![
                (
                    source("FIRST", "first.csv"),
                    Error::Source(TextTableError::Csv(
                        csv_source::parse(b"ID,ID\nx,y").unwrap_err(),
                    )),
                ),
                (
                    source("IGNORED", "outer.csv"),
                    Error::Sources(vec![(
                        source("LAST", "last.parquet"),
                        Error::ParquetSource(parquet::Error::Unsupported {
                            field: "F\0β".into(),
                            stored_type: "\0\"\\β".repeat(4000),
                        }),
                    )]),
                ),
            ]);
            let Error::Sources(errors) = &error else {
                panic!()
            };
            let Error::Sources(nested) = &errors[1].1 else {
                panic!()
            };
            let Error::ParquetSource(parquet::Error::Unsupported { field, stored_type }) =
                &nested[0].1
            else {
                panic!()
            };
            let pointers = (
                field.as_ptr(),
                stored_type.as_ptr(),
                nested[0].0.path.as_ptr(),
            );
            let finding = match &nested[0].1 {
                Error::ParquetSource(error) => {
                    error.finding(&nested[0].0.name, &nested[0].0.path).unwrap()
                }
                _ => panic!(),
            };
            let SourceFinding::Parquet {
                finding:
                    ParquetFinding::Unsupported {
                        field: borrowed_field,
                        stored_type: borrowed_type,
                    },
                path,
                ..
            } = finding
            else {
                panic!()
            };
            assert_eq!(
                (
                    borrowed_field.as_ptr(),
                    borrowed_type.as_ptr(),
                    path.as_ptr()
                ),
                pointers
            );
            assert!(
                source_diagnostics::findings(&errors[0].1, Some(&errors[0].0))
                    .unwrap()
                    .is_some()
            );
            assert!(
                source_diagnostics::findings(&nested[0].1, Some(&nested[0].0))
                    .unwrap()
                    .is_some()
            );
            let actual = source_diagnostics::findings(&error, None).unwrap().unwrap();
            assert_eq!(
                actual
                    .iter()
                    .map(|finding| finding.spec_paths[0].as_str())
                    .collect::<Vec<_>>(),
                ["input.FIRST.path", "input.LAST.path"]
            );
            let portable = projection::findings(&error, None).unwrap().unwrap();
            assert_eq!(portable[1].context["stored_type"], *stored_type);
            assert_eq!(portable[1].context["field"], *field);
            assert_eq!(
                (
                    field.as_ptr(),
                    stored_type.as_ptr(),
                    nested[0].0.path.as_ptr()
                ),
                pointers
            );
            assert_eq!(
                source_diagnostics::findings(&error, None).unwrap().unwrap(),
                actual
            );
        }

        #[test]
        fn missing_identity_is_explicit_and_resource_barriers_preserve_original_error_precedence() {
            let semantic = Error::ParquetSource(parquet::Error::Malformed);
            assert_eq!(
                source_diagnostics::findings(&semantic, None),
                Err(source_diagnostics::SourceProjectionError::InvalidContext)
            );
            assert_eq!(
                projection::failure(&semantic, None),
                projection::Rejection::Fault(yamaa_core::application_issue::rejected(
                    yamaa_core::application_issue::Step::Ingest,
                    yamaa_core::application_issue::Fault::DiagnosticContext,
                ))
            );
            let declaration = source("SRC", "source.csv");
            for error in [
                Error::Source(TextTableError::Csv(csv_source::Error::Limit {
                    resource: "source_bytes",
                    limit: yamaa_core::limits::Limits::SOURCE_BYTES,
                    actual: Some(yamaa_core::limits::Limits::SOURCE_BYTES + 1),
                })),
                Error::TypedSource(typed_csv::Error::Typing(
                    yamaa_core::typed_csv::Error::InvalidDeclarations,
                )),
                Error::ParquetSource(parquet::Error::ResourceLimit {
                    resource: "source_bytes",
                    limit: yamaa_core::limits::Limits::SOURCE_BYTES,
                    actual: Some(yamaa_core::limits::Limits::SOURCE_BYTES + 1),
                }),
                Error::ParquetSource(parquet::Error::Representation {
                    representation: "Utf8 offset width",
                }),
                Error::ParquetSource(parquet::Error::Unavailable { codec: i64::MAX }),
            ] {
                assert_eq!(source_diagnostics::findings(&error, None), Ok(None));
                let collected = Error::Sources(vec![
                    (
                        declaration.clone(),
                        Error::ParquetSource(parquet::Error::Unsupported {
                            field: "F".into(),
                            stored_type: "late".repeat(1000),
                        }),
                    ),
                    (declaration.clone(), error),
                ]);
                assert_eq!(source_diagnostics::findings(&collected, None), Ok(None));
                assert_eq!(
                    projection::failure(&collected, None),
                    projection::Rejection::Fault(yamaa_core::application_issue::rejected(
                        yamaa_core::application_issue::Step::Ingest,
                        yamaa_core::application_issue::Fault::SourceCollectionBoundary,
                    ))
                );
            }
            assert_eq!(
                source_diagnostics::findings(&Error::Sources(vec![]), None),
                Ok(Some(vec![]))
            );
        }

        #[test]
        fn complete_projection_preserves_full_oversized_value_and_both_findings() {
            let original = "9".repeat(1_400_000);
            let error = Error::Sources(vec![
                (
                    source("FIRST", "first.csv"),
                    Error::ParquetSource(parquet::Error::Malformed),
                ),
                (
                    source("LAST", "last.csv"),
                    Error::TypedSource(typed_csv::Error::Typing(
                        yamaa_core::typed_csv::Error::FieldParse {
                            field: "F".into(),
                            target: ColumnType::Int,
                            value: original.clone(),
                        },
                    )),
                ),
            ]);
            let diagnostics = source_diagnostics::findings(&error, None).unwrap().unwrap();
            assert_eq!(diagnostics.len(), 2);
            let portable = projection::findings(&error, None).unwrap().unwrap();
            assert_eq!(portable.len(), 2);
            assert_eq!(portable[1].context["value"], original);
            assert_eq!(
                projection::failure(&error, None),
                projection::Rejection::Invalid(portable)
            );
            let Error::Sources(errors) = &error else {
                panic!("retained collection")
            };
            let Error::TypedSource(typed_csv::Error::Typing(
                yamaa_core::typed_csv::Error::FieldParse { value, .. },
            )) = &errors[1].1
            else {
                panic!("retained value")
            };
            assert_eq!(value, &original);
        }

        fn nested(mut error: Error, depth: usize) -> Error {
            for _ in 0..depth {
                error = Error::Sources(vec![(source("DEEP", "deep.parquet"), error)]);
            }
            error
        }
        #[cfg(feature = "arrow")]
        fn opaque() -> Error {
            Error::ParquetSource(parquet::Error::Table(
                yamaa_adapters::column_table::TableError::Arrow(
                    arrow_schema::ArrowError::ComputeError(
                        "original opaque codec allocation".into(),
                    ),
                ),
            ))
        }
        #[cfg(feature = "arrow")]
        fn opaque_text(error: &Error) -> &str {
            let Error::ParquetSource(parquet::Error::Table(
                yamaa_adapters::column_table::TableError::Arrow(
                    arrow_schema::ArrowError::ComputeError(text),
                ),
            )) = error
            else {
                panic!("original opaque allocation")
            };
            text
        }
        fn wide(width: usize) -> Vec<(SourceDeclaration, Error)> {
            (0..width)
                .map(|_| {
                    (
                        source("WIDE", "wide.parquet"),
                        Error::ParquetSource(parquet::Error::Unsupported {
                            field: "F".into(),
                            stored_type: "captured physical metadata".into(),
                        }),
                    )
                })
                .collect()
        }

        #[test]
        fn wide_nested_semantic_report_preserves_every_supplied_finding() {
            let error = nested(Error::Sources(wide(50_000)), 2);
            let actual = source_diagnostics::findings(&error, None).unwrap().unwrap();
            assert_eq!(actual.len(), 50_000);
            for finding in &actual {
                assert_eq!(finding.spec_paths, ["input.WIDE.path"]);
            }
            let Error::Sources(outer) = &error else {
                panic!("retained outer")
            };
            assert_eq!(outer[0].0.name, "DEEP");
            assert_eq!(
                source_diagnostics::findings(&error, None).unwrap().unwrap(),
                actual
            );
        }

        #[test]
        #[cfg(feature = "arrow")]
        fn late_opaque_after_wide_nested_report_keeps_original_allocation_and_precedence() {
            let mut children = wide(50_000);
            let original = opaque();
            let pointer = opaque_text(&original).as_ptr();
            children.push((source("OPAQUE", "opaque.parquet"), original));
            let error = nested(Error::Sources(children), 2);
            assert_eq!(source_diagnostics::findings(&error, None), Ok(None));
            let Error::Sources(outer) = &error else {
                panic!()
            };
            let Error::Sources(middle) = &outer[0].1 else {
                panic!()
            };
            let Error::Sources(children) = &middle[0].1 else {
                panic!()
            };
            assert_eq!(children.len(), 50_001);
            assert_eq!(opaque_text(&children[50_000].1).as_ptr(), pointer);
            assert_eq!(
                opaque_text(&children[50_000].1),
                "original opaque codec allocation"
            );
            assert_eq!(source_diagnostics::findings(&error, None), Ok(None));
            assert_eq!(opaque_text(&children[50_000].1).as_ptr(), pointer);
        }

        #[test]
        #[cfg(feature = "arrow")]
        fn reached_opaque_barrier_stops_before_later_deep_and_oversized_semantic_facts() {
            let original = opaque();
            let pointer = opaque_text(&original).as_ptr();
            let late = Error::TypedSource(typed_csv::Error::Typing(
                yamaa_core::typed_csv::Error::FieldParse {
                    field: "F".into(),
                    target: ColumnType::Int,
                    value: "9".repeat(1_400_000),
                },
            ));
            let error = Error::Sources(vec![
                (source("FIRST", "first.parquet"), original),
                (source("LATER", "later.csv"), nested(late, 129)),
            ]);
            assert_eq!(source_diagnostics::findings(&error, None), Ok(None));
            assert_eq!(source_diagnostics::findings(&error, None), Ok(None));
            assert_eq!(
                projection::failure(&error, None),
                projection::Rejection::Fault(yamaa_core::application_issue::rejected(
                    yamaa_core::application_issue::Step::Ingest,
                    yamaa_core::application_issue::Fault::SourceCollectionBoundary,
                ))
            );
            let Error::Sources(children) = &error else {
                panic!()
            };
            assert_eq!(opaque_text(&children[0].1).as_ptr(), pointer);
        }

        #[test]
        #[cfg(feature = "arrow")]
        fn deep_borrowed_reports_keep_full_facts_and_later_opaque_errors_preserve_precedence() {
            let leaf = Error::ParquetSource(parquet::Error::Unsupported {
                field: "F".into(),
                stored_type: "held physical type".into(),
            });
            let Error::ParquetSource(parquet::Error::Unsupported { stored_type, .. }) = &leaf
            else {
                panic!("retained metadata")
            };
            let pointer = stored_type.as_ptr();
            let admitted = nested(leaf, 129);
            let diagnostics = source_diagnostics::findings(&admitted, None)
                .unwrap()
                .unwrap();
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(diagnostics[0].spec_paths, ["input.DEEP.path"]);
            let held = Error::Sources(vec![
                (source("DEEP", "deep.parquet"), admitted),
                (source("LATE", "late.parquet"), opaque()),
            ]);
            assert_eq!(source_diagnostics::findings(&held, None), Ok(None));
            assert_eq!(
                projection::failure(&held, None),
                projection::Rejection::Fault(yamaa_core::application_issue::rejected(
                    yamaa_core::application_issue::Step::Ingest,
                    yamaa_core::application_issue::Fault::SourceCollectionBoundary,
                ))
            );
            let Error::Sources(children) = &held else {
                panic!("retained collection")
            };
            let mut leaf = &children[0].1;
            for _ in 0..129 {
                let Error::Sources(child) = leaf else {
                    panic!("retained nested metadata")
                };
                leaf = &child[0].1;
            }
            let Error::ParquetSource(parquet::Error::Unsupported { stored_type, .. }) = leaf else {
                panic!("retained metadata")
            };
            assert_eq!(stored_type.as_ptr(), pointer);
            assert_eq!(stored_type, "held physical type");
        }
    }
    mod typed_csv {
        // What a build reads from a typed CSV input, refused or not, is a case
        // of the application suite's `fixtures/diagnostics/typed_csv_sources.json`.
        // The tests left here compare what no case states: a float's bits, a
        // temporal value's collected precision, the typed table's schema, and
        // inputs generated in the test.
        use yamaa_adapters::typed_csv;
        use yamaa_core::{
            table::{TableAccess, ValueRef},
            value::ColumnType as T,
        };
        /// Seam: the bits of a negative zero and the collected precision of a
        /// date and a datetime, which an output cell does not hold.
        #[test]
        fn declared_types_keep_exact_i64_float_bits_temporal_precision_and_undeclared_text() {
            let data=b"I,F,D,T,RAW\n9007199254740993,-0.0,2024-03-02,2024-03-02T00:00:00,0012\n,1.5,2024-01-01,2024-03-02T01:02:03,NA\n";
            let table = typed_csv::parse(
                data,
                &[
                    ("I".into(), T::Int),
                    ("F".into(), T::Float),
                    ("D".into(), T::Date),
                    ("T".into(), T::DateTime),
                ],
            )
            .unwrap();
            assert_eq!(table.cell(0, 0).unwrap(), ValueRef::Int(9007199254740993));
            let ValueRef::Float(value) = table.cell(0, 1).unwrap() else {
                panic!()
            };
            assert_eq!(value.get().to_bits(), (-0.0f64).to_bits());
            let ValueRef::Date(value) = table.cell(0, 2).unwrap() else {
                panic!()
            };
            assert_eq!(value.to_string(), "2024-03-02");
            assert_eq!(
                value.collected_precision(),
                yamaa_core::temporal::Precision::Day
            );
            let ValueRef::DateTime(value) = table.cell(0, 3).unwrap() else {
                panic!()
            };
            assert_eq!(
                value.collected_precision(),
                yamaa_core::temporal::Precision::Second
            );
            assert_eq!(table.cell(0, 4).unwrap(), ValueRef::Str("0012"));
            assert_eq!(table.cell(1, 0).unwrap(), ValueRef::Missing);
            assert_eq!(table.cell(1, 4).unwrap(), ValueRef::Str("NA"));
        }

        /// Seam: integers behind 5,000 generated zeros, which a case would
        /// write out in full.
        #[test]
        fn retired_python_integer_ingestion_keeps_every_i64_through_long_zero_prefixes() {
            // Original test_source.py operands: CSV admission must not inherit a host
            // integer-string digit ceiling or round the values through binary64.
            let expected = [0_i64, 1, 9_007_199_254_740_993, i64::MAX, i64::MIN];
            let mut input = String::from("VALUE\n");
            for value in expected {
                input.push(if value < 0 { '-' } else { '+' });
                input.push_str(&"0".repeat(5000));
                input.push_str(&value.unsigned_abs().to_string());
                input.push('\n');
            }
            let table = typed_csv::parse(input.as_bytes(), &[("VALUE".into(), T::Int)]).unwrap();
            assert_eq!(table.row_count(), expected.len());
            for (row, value) in expected.into_iter().enumerate() {
                assert_eq!(table.cell(row, 0).unwrap(), ValueRef::Int(value));
            }
        }

        /// Seam: the typed table's schema, each stored field's name, order and
        /// kind, which a case's output columns, declared by its specification,
        /// do not show. The values the same bytes read are the cases
        /// `typed_csv_sources.json::a_typed_csv_keeps_the_retired_python_missingness_and_civil_time`,
        /// `a_typed_csv_with_a_header_alone_reads_no_rows` and
        /// `a_quoted_empty_typed_field_reads_as_missing`.
        #[test]
        fn retired_python_typed_csv_keeps_original_field_order_and_declared_kinds() {
            let declared = [
                ("AGE".into(), T::Int),
                ("SCORE".into(), T::Float),
                ("DATE".into(), T::Date),
                ("MOMENT".into(), T::DateTime),
            ];
            let table = typed_csv::parse(
                b"ID,AGE,SCORE,DATE,MOMENT,EMPTY,TEXT\n\
007,42,1.5,2025-01-02,2025-01-02T03:04,,\"\"\n\
008,,.Inf,2025-02-03,2025-02-03T04:05:06,NA,unknown\n",
                &declared,
            )
            .unwrap();
            assert_eq!(
                table
                    .schema()
                    .columns()
                    .iter()
                    .map(|column| (column.name.as_str(), column.kind))
                    .collect::<Vec<_>>(),
                [
                    ("ID", T::Str),
                    ("AGE", T::Int),
                    ("SCORE", T::Float),
                    ("DATE", T::Date),
                    ("MOMENT", T::DateTime),
                    ("EMPTY", T::Str),
                    ("TEXT", T::Str),
                ]
            );
            let empty = typed_csv::parse(b"ID,AGE\n", &declared[..1]).unwrap();
            assert_eq!(empty.schema().columns()[0].kind, T::Str);
            assert_eq!(empty.schema().columns()[1].kind, T::Int);
        }
    }
}
