# Reject a site name stored in another encoding

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-source-invalid-text.html)

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
