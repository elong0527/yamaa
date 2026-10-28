# Read the Times and Weights a Data Frame Library Writes to Parquet

**Goal:** carry the date and time of first and last exposure (`TRTSDTM`,
`TRTEDTM`) and the baseline weight (`WEIGHTBL`) into ADSL from a Parquet
export, each value as the stored data means it.

**Input:** one Parquet file, `input/adsl_raw.parquet`, with one record per
subject, as pyarrow writes a data frame by default: the first exposure
counted in nanoseconds, the last exposure in milliseconds, and the weight as
a single-precision number.

**Variables:**

- `TRTSDTM` and `TRTEDTM` are the date and time of first and last exposure,
  to the second; missing when the subject was never treated, or, for the
  last exposure, while treatment continues.
- `WEIGHTBL` is the baseline weight in kilograms, the exact number stored;
  missing when not measured.

**Note:** a time stored to the millisecond or the nanosecond reads as the
same date and time as one stored to the microsecond, as long as it falls on
a whole second. A single-precision number holds fewer digits than an
ordinary one, so a weight typed as 81.3 is stored as 81.30000305175781 and
arrives as exactly that number, not rounded back to 81.3.
