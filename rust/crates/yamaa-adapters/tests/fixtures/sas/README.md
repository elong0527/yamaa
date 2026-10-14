# Independent SAS source fixtures

`build_xport.py` writes transport files directly from documented 80-byte
records and 140-byte NAMESTR layouts, with rational IBM numbers. It imports no
candidate parser and no ReadStat code. Literal rows are `ID` 2, 1, 3 in that
order; text ` café`, blank, `.` in Windows-1252; `NUM` 1.5, `.Z`, -2; `DAY`
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
