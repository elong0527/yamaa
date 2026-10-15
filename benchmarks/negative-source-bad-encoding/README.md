# Reject Bad Encoding

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-source-bad-encoding.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the site name (`SITENM`) onto a subject listing.

**Input:** collected demographics carrying `SITENM`.

**Variables:**

- `SITENM` would be the name of the site the subject enrolled
  at, taken from the collected value.

One stored site name holds a byte written in an older encoding,
which is not valid UTF-8 text. Replacing the byte with a
substitute character would store a name the study never recorded,
and dropping it would store a different name again, so the run is
rejected while the file is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Export the listing as UTF-8, so every collected character keeps the value it
was entered with. The second site name then stores its accented letter (o
with circumflex, U+00F4) as the two bytes `C3 B4` instead of the single byte
`F4` that the older encoding wrote.

Converting a file after the fact is safe only while the original encoding is
known. A study that stores names in an unstated encoding cannot say which
letters it collected.
