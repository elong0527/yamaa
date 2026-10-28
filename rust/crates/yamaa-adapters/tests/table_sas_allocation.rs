//! Table contracts: SAS allocation.
//!
//! The seam is `sas_source::parse` over SAS7BDAT bytes built by code, read
//! again by a child process under a bounded address space, which neither the
//! benchmark reader nor the host fixtures reach.
mod table {
    #[cfg(feature = "readstat")]
    mod sas_allocation {
        //! The SAS7BDAT reader sizes its buffers from the columns it validates.
        //! Files are built at test time from the documented layout, so the
        //! expected cells stay independent of the reader.
        use crate::support::sas7bdat::{Cell, Dataset, Field, Kind, Layout, Page, Pointer, MIX};
        use yamaa_adapters::sas_source;
        use yamaa_core::{
            specification::{SourceDeclaration, SourceProfile},
            table::{TableAccess, ValueRef},
            value::FiniteFloat,
        };

        /// Set in the child process that reads the files under a bounded
        /// address space. Only Linux starts one.
        #[cfg(target_os = "linux")]
        const CHILD: &str = "YAMAA_SAS_ALLOCATION_CHILD";

        /// One record of `X` and `T` on a mix page, whose column-attributes
        /// subheader holds a third slot beyond the two columns, with the
        /// widest width a slot can declare. No row reads that slot.
        fn wide_slot_beyond_the_columns(layout: Layout) -> Vec<u8> {
            let mut dataset = Dataset::new(layout, &[("X", Kind::Num, 8), ("T", Kind::Chr, 8)]);
            dataset.total = 1;
            dataset.mix_rows = 1;
            let row = dataset.row(&[Cell::Num(2.5), Cell::Text(b"kept")]);
            let mut slots = dataset.fields.clone();
            slots.push(Field {
                name: String::new(),
                kind: Kind::Chr,
                offset: 0,
                width: u32::MAX,
            });
            let mut pointers = dataset.metadata();
            *pointers.last_mut().unwrap() = Pointer::metadata(layout.column_attrs(&slots));
            dataset.file(&[Page::new(MIX, pointers, vec![row])])
        }

        /// Every layout's file reads its one record.
        fn read_every_layout() {
            let source = SourceDeclaration {
                name: "SRC".into(),
                path: "built.sas7bdat".into(),
                types: vec![],
                profile: SourceProfile::Sas7bdat,
                encoding: None,
                byte_order_mark: false,
                empty_string_present: false,
                ordinal: None,
            };
            for layout in [Layout::LE32, Layout::LE64, Layout::BE32, Layout::BE64] {
                let table = sas_source::parse(&wide_slot_beyond_the_columns(layout), &source)
                    .unwrap_or_else(|error| panic!("{layout:?}: {error:?}"));
                assert_eq!(table.row_count(), 1, "{layout:?}");
                assert_eq!(
                    table.cell(0, 0).unwrap(),
                    ValueRef::Float(FiniteFloat::new(2.5).unwrap())
                );
                assert_eq!(table.cell(0, 1).unwrap(), ValueRef::Str("kept"));
            }
        }

        #[test]
        fn a_column_attribute_slot_beyond_the_columns_sizes_no_row_buffer() {
            // ReadStat sized its row text buffer at four bytes per byte of the
            // widest attribute slot, validated or not: about 16 GiB for this
            // slot (found by fuzzing, #2928). Under memory overcommit such a
            // request can succeed untouched, so a plain read cannot tell.
            read_every_layout();
            // On Linux a child process reads the files again with its address
            // space bounded to 2 GiB. A request that large fails there, so a
            // reader that made it would stop with an internal error. The child
            // only reads.
            #[cfg(target_os = "linux")]
            if std::env::var_os(CHILD).is_none() {
                let test = concat!(
                    module_path!(),
                    "::a_column_attribute_slot_beyond_the_columns_sizes_no_row_buffer"
                );
                let output = std::process::Command::new("/bin/sh")
                    .args(["-c", "ulimit -v 2097152 && exec \"$0\" \"$@\""])
                    .arg(std::env::current_exe().unwrap())
                    .args(["--exact", test.split_once("::").unwrap().1])
                    .arg("--test-threads=1")
                    .env(CHILD, "1")
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{output:?}");
                // The child ran this test rather than matching nothing.
                let stdout = String::from_utf8_lossy(&output.stdout);
                assert!(stdout.contains("1 passed"), "{stdout}");
            }
        }
    }
}
