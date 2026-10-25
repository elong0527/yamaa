# Read a Dataset-JSON File

**Goal:** carry the actual treatment (`TRT01A`), age (`AGE`), baseline
height (`HEIGHTBL`) and the date and time of first exposure (`TRTSDT`,
`TRTSDTM`) into ADSL from a Dataset-JSON file, each value as the stored data
means it.

**Input:** one `spec.yaml` reading `input/adsl_raw.json`, a Clinical Data
Interchange Standards Consortium (CDISC) Dataset-JSON 1.1 file in its
single-object form, with one subject-level record per subject. Each column
of the file states its own data type.

**Variables:**

- `TRT01A` is the actual treatment; missing when the subject was never
  treated, which the file stores as null.
- `AGE` is the age in whole years.
- `HEIGHTBL` is the baseline height in centimetres, which the file stores as
  decimal text; missing when not measured.
- `TRTSDT` is the date of first exposure; missing when the subject was never
  treated.
- `TRTSDTM` is the date and time of first exposure; missing when the subject
  was never treated.

**Note:** each value takes the type its column states: whole numbers stay
whole, a decimal written as text is read as a number, and dates and times
are read from their ISO 8601 text, a time to the minute gaining zero
seconds. A null is missing. The file's labels, lengths and display formats,
and its request that a receiving system convert dates to SAS numbers,
decide nothing.
