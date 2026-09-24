# Reject Unterminated Quote

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-source-unterminated-quote.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the investigator comment (`CMNT`) for each subject,
one record per subject.

**Input:** collected subject listing holding the investigator
comment (`COMMENT`).

**Variables:**

- `CMNT` would be the comment the investigator recorded about the
  subject, copied from `COMMENT`.

The last comment opens quoting that it never closes, so the file
says its text continues past the end of the file. Where that record
ends, and therefore how many subjects the listing holds, depends on
how far a reader chooses to read, so the run is rejected while
reading the listing and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Close the quoting right after the text the investigator recorded; if the
producer cut the comment short, take the full text from the source rather
than guessing it:

    STUDYID,USUBJID,COMMENT
    CTX,CTX-01,Dose reduced
    CTX,CTX-02,"Visit missed"

A quotation mark that belongs to the text is doubled inside the quoting,
as in `"Subject said ""felt fine"""`. A comment holding a separator, a
quotation mark, or a line break stays quoted; one holding none of them
needs no quoting at all.
