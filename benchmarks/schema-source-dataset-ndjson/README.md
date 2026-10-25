# Read a Newline-Delimited Dataset-JSON File

**Goal:** carry the visit number (`AVISITN`), the standardized result
(`AVAL`), the collected result as written (`AVALC`) and the date and time of
measurement (`ADTM`) into ADVS from a vital signs file in Dataset-JSON's
newline-delimited form.

**Input:** one `spec.yaml` reading `input/vs.ndjson`, a Clinical Data
Interchange Standards Consortium (CDISC) Dataset-JSON 1.1 file whose first
line describes the dataset and whose every later line is one record. Each
column states its own data type.

**Variables:**

- `AVISITN` is the visit number, which the file stores as a floating-point
  number and the study reads as a whole number.
- `AVAL` is the result in standard units; missing when not measured.
- `AVALC` is the collected result exactly as the file writes it, trailing
  zeros included; missing when not measured.
- `ADTM` is the date and time of the measurement; missing when not
  measured.

**Note:** a value takes the type its column states unless the study narrows
it: a floating-point visit number is accepted as a whole number only when it
is exactly whole, and a decimal can be kept as the text the sender wrote
rather than read as a number. Dates and times are read from their ISO 8601
text, a time to the minute gaining zero seconds, and a null is missing.
