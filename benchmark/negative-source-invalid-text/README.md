# Reject a site name stored in another encoding

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-source-invalid-text.html) [![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Lifecycle:** reviewed - has discussion comments or GitHub issues.

**Goal:** carry the site name (`SITENM`) onto a subject listing.

**Input:** collected demographics carrying `SITENM`.

**Variables:**

- `SITENM` would be the name of the site the subject enrolled
  at, taken from the collected value.

A stored name holds bytes in an older encoding that spell no text
in the reading: replacing the byte with a substitute character
would store a name the study never recorded, and dropping it would
store a different name again. The run is rejected while the stored
file is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Export the listing as UTF-8, so every collected character keeps the value it
was entered with, including the accented letter (o with circumflex, U+00F4):

    CTX,CTX-02,Hopital Saint-Antoine

Converting a file after the fact is safe only while the original encoding is
known. A study that stores names in an unstated encoding cannot say which
letters it collected.
