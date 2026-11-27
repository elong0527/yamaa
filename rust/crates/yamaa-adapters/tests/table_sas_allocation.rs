//! Table contracts: SAS allocation.
//!
//! The seam is `sas_source::parse` over SAS7BDAT and transport bytes built by
//! code, read again by a child process under a bounded address space, which
//! neither the benchmark reader nor the host fixtures reach.
mod table {
    #[cfg(feature = "readstat")]
    mod sas_allocation {
        //! The SAS readers size their buffers from the columns they validate
        //! and the bytes the file holds. Files are built at test time from the
        //! documented layouts, so the expected cells stay independent of the
        //! reader.
        use crate::support::sas7bdat::{Cell, Dataset, Field, Kind, Layout, Page, Pointer, MIX};
        use yamaa_adapters::sas_source::{self, Error};
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
                fixed_width: None,
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
            // reader that made it would stop with an internal error.
            #[cfg(target_os = "linux")]
            read_again_in_a_bounded_child(
                "a_column_attribute_slot_beyond_the_columns_sizes_no_row_buffer",
            );
        }

        /// A version 5 or 8 transport member of 32,768 character fields of
        /// 65,535 bytes each, a row of 2,147,450,880 bytes, followed by no
        /// observation record. `observations` is the observation header's
        /// text from byte 48.
        fn transport_wider_than_its_input(version: u8, observations: &str) -> Vec<u8> {
            let names: Vec<String> = (1..=32_768).map(|index| format!("V{index}")).collect();
            let fields: Vec<(&str, u16)> =
                names.iter().map(|name| (name.as_str(), u16::MAX)).collect();
            crate::support::xport::transport(version, &fields, observations, &[])
        }

        /// Without a count the member reads no rows; a count of one record
        /// is not met by a member that holds none.
        fn read_wide_transports() {
            let source = SourceDeclaration {
                name: "SRC".into(),
                path: "built.xpt".into(),
                types: vec![],
                profile: SourceProfile::Xport,
                encoding: Some("windows-1252".into()),
                byte_order_mark: false,
                fixed_width: None,
                empty_string_present: false,
                ordinal: None,
            };
            let bytes = transport_wider_than_its_input(5, &"0".repeat(30));
            let table = sas_source::parse(&bytes, &source).unwrap();
            assert_eq!(
                (table.row_count(), table.schema().columns().len()),
                (0, 32_768),
                "version 5 without a count"
            );
            let bytes = transport_wider_than_its_input(8, &format!("{:15}", 1));
            let outcome = sas_source::parse(&bytes, &source);
            assert!(
                matches!(outcome, Err(Error::Invalid("malformed"))),
                "version 8 counting one record: {outcome:?}"
            );
        }

        #[test]
        fn a_transport_row_wider_than_the_input_sizes_no_row_buffer() {
            // ReadStat allocated two buffers of the row length the NAMESTR
            // records declare, 2 GiB each here, and filled one with blanks
            // before it read a record, from a member of under 5 MiB that holds
            // no observation bytes (#5073). Under memory overcommit the
            // parent can still make those requests.
            read_wide_transports();
            // A child process bounded to 2 GiB of address space cannot, so a
            // reader that made them would stop with an internal error.
            #[cfg(target_os = "linux")]
            read_again_in_a_bounded_child(
                "a_transport_row_wider_than_the_input_sizes_no_row_buffer",
            );
        }

        /// Runs `test`, a test of this module, again in a child process whose
        /// address space is bounded to 2 GiB. The child only reads.
        #[cfg(target_os = "linux")]
        fn read_again_in_a_bounded_child(test: &str) {
            if std::env::var_os(CHILD).is_some() {
                return;
            }
            let test = format!("{}::{test}", module_path!());
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
