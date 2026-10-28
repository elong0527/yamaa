# Reject Text Read as a Whole Number from a SAS Transport File

**Goal:** carry the reported adverse event term (`AETERM`) and its sequence
number (`AESEQ`) onto the adverse events domain from a SAS transport file.

**Input:** reported adverse events in `input/ae_raw.xpt`, a version 5 SAS
transport file with one record per reported event. The sequence number is
stored as a number and the reported term as text.

**Variables:**

- `DOMAIN` is `AE` on every record.
- `AESEQ` would number each subject's events in whole numbers.
- `AETERM` would be the term as reported. The rejected file produces no
  output value.

**Note:** a SAS file stores every field either as text or as a number, and a
field stored as text is never read as a number. The reported term is
declared a whole number, so the file is rejected while it is read, naming
the term's field, before any record is derived; no artifact is accepted.

## How to fix

The reported term is verbatim text, so read it as the text it is stored as
by declaring no type for it; only the sequence number needs narrowing to a
whole number:

```yaml
input:
  AE_RAW:
    path: input/ae_raw.xpt
    types: {AESEQ: int}
    encoding: windows-1252
```

When a field stored as text does hold digits meant as a number, ask the
sender to store it as a number, or read it as text and convert it in the
column that needs the number.
