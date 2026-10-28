# Independent SAS source fixtures

`python -m tools.staging.build_xport_fixtures` writes transport files directly from documented 80-byte
records and 140-byte NAMESTR layouts, with rational IBM numbers. It imports no
candidate parser and no ReadStat code. Literal rows are `ID` 2, 1, 3 in that
order; text ` caf\xe9`, blank, `.` in Windows-1252; `NUM` 1.5, `.Z`, -2; `DAY`
0, 3653, -1; `STAMP` 0, -1, 1. Version 8 uses `LONG_TEXT_NAME`. Each multi-member
fixture contains the valid second member's descriptor and data records rather
than a marker-only mutation. `precision.xpt` stores exact integer 2^53+1; ReadStat decodes it to binary64
2^53, and explicit narrowing must reject that precision loss. `decimal.xpt`
stores binary64 0.1 as an exactly representable IBM float and stays float.
`member-text.xpt` stores a complete MEMBER-header-looking character value
without a following dataset descriptor; it is ordinary text, not a member.
`missing.xpt` contains ordinary missing, underscore, and A through Z tags.

`datetime.sas7bdat` is copied unchanged from tidyverse/haven commit
`9126825a2813e37b66023b4979a037dfc74d7217`, path
`tests/testthat/sas/datetime.sas7bdat`, under `LICENSE-haven.txt`. Upstream
`tests/testthat/test-haven-sas.R` independently specifies its first row as
VAR1 2015-02-02T14:42:12, VAR2/VAR3/VAR4 2015-02-02, VAR5 52932 seconds.
Its display formats must not infer types in yamaa.

`rle.sas7bdat`, `rdc.sas7bdat`, and `compressed-truth.csv` are copied unchanged
from pandas-dev/pandas commit `d1a2fa7edf06aaf1e914b3202af25923c77e3a24`, paths
`pandas/tests/io/sas/data/test2.sas7bdat`, `test3.sas7bdat`, and
`test_sas7bdat_1.csv`, under `LICENSE-pandas.txt`. The CSV is upstream independent
truth for these two compression variants. `test_sas7bdat.py` identifies the
RLE/RDC corruption offsets used in the test, independently of this reader.

The deleted-record controls edit `datetime.sas7bdat`. Byte 1040 set to 0x80
adds the deleted-rows flag to its mix page (type 0x0200). That page's bitmap
starts at byte 3764: page offset 184 for the first record, four 40-byte
records, then the 2396 bytes of free space recorded at byte 1036. The bitmap
holds zeros, so the flag alone deletes nothing. Byte 4668 set to 1 counts one
deleted record in its row-size subheader at offset 4640, beside its four stored
records; alone it leaves that deletion unmarked and is refused with cause
`deleted_records`. With the flag, the count and byte 3764 set to 0x40, the
second record is deleted and skipped. In `rle.sas7bdat`, byte 66840 set to 0x05
marks the first compressed row deleted and byte 130620 set to 1 counts it; the
other nine records equal the pandas truth. No committed SAS dataset holds a
record deleted in place: none has a page flagged `0x0080`, a pointer with code
`0x05`, `0x09` or `0x0d`, or a nonzero deleted count. The bitmap layout is
therefore checked against these edits and the test builders, not against a file
that SAS wrote.

Expected host CSV bytes and typed tables are literal authored truth. No golden
file is written by the candidate parser. New dependency licenses travel with
source packages; fixture licenses accompany their binary source inventory.

`utf8.xpt` has the same literal rows as v5 with UTF-8 text.
`integer-bounds.xpt` stores -2^63, zero, 2^53, and 2^63-1024;
`integer-overflow.xpt` stores 2^63. `unsupported-compression.sas7bdat` changes
only the first metadata compression signature from SASYZCRL to UNKNOWN!.
`rdc-big-endian.sas7bdat` comes from the same pandas commit, `test11.sas7bdat`.
The RDC overlap control follows upstream `test_rdc_overlapping_back_reference`: 
change bytes 120922:120924 from 8d00 to 8100; expected Column11 repeats the
last four bytes of the independently authored first-row Column9 (0.621).

`utf8-concentration.sas7bdat` is independently assembled by
`tests/support/sas7bdat.rs::utf8_concentration`, using the documented layout
builder described in that module. It holds ID 1 as an eight-byte binary64
number and UNIT `\u00b5mol/L` as a twelve-byte, space-padded UTF-8 character
field. Header byte 70 is encoding code 20 (UTF-8). The helper writes one
1024-byte header and one 4096-byte MIX page. It imports neither yamaa nor
ReadStat. The same bytes are the inputs of `schema-source-sas7bdat-encoding`
and `negative-sas7bdat-encoding-mismatch`; the adapter codec test compares
all three files to the independent builder. To recreate them, compile a
small Rust program that imports `tests/support/sas7bdat.rs` by path and
writes `sas7bdat::utf8_concentration()` to those three paths. No output
expectation is obtained from the reader.

The pandas big-endian RDC fixture records encoding code 29 (ISO-8859-1),
while the little-endian compressed fixtures record code 62 (Windows-1252).
The shared host truth declares those encodings per file; its cell values
remain the unchanged upstream CSV truth.
