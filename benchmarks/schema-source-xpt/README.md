# Read a SAS Transport File

**Goal:** carry the actual treatment (`TRT01A`), age (`AGE`), baseline
height (`HEIGHTBL`) and the date and time of first exposure (`TRTSDT`,
`TRTSDTM`) into ADSL from a SAS transport file, each value as the stored
data means it.

**Input:** one `spec.yaml` reading `input/adsl_raw.xpt`, a version 5 SAS
transport file with one subject-level record per subject, as SAS writes it:
text padded with blanks in the Windows-1252 encoding, numbers as IBM
floating-point values, and dates and times as SAS numeric dates and
datetimes.

**Variables:**

- `TRT01A` is the actual treatment; missing when the subject was never
  treated, which the file stores as blanks.
- `AGE` is the age in whole years.
- `HEIGHTBL` is the baseline height in centimetres, as stored; missing when
  not measured.
- `TRTSDT` and `TRTSDTM` are the date, and the date and time, of first
  exposure; missing when the subject was never treated.

**Note:** text loses only the blanks that pad it on the right, and text of
blanks alone is missing. A stored number reads as a floating-point number
unless the spec narrows it: `AGE` to a whole number, and `TRTSDT` and
`TRTSDTM`, which the file counts in days and in seconds from 1 January 1960,
to a date and to a date and time. A SAS missing value reads as missing, and
the display formats the file carries decide nothing.
