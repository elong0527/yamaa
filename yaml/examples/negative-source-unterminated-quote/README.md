# Reject a comment whose quoted text never closes

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-source-unterminated-quote.html)

**Goal:** carry the investigator comment (`CMNT`) for each subject,
one record per subject.

**Input:** collected subject listing holding the investigator
comment (`COMMENT`).

**Variables:**

- `CMNT` would be the comment the investigator recorded about the
  subject, copied from `COMMENT`.

The result would carry `CMNT` beside the study and subject
identifiers, but no row is produced: the last comment opens quoting
that it never closes, so the file says its text continues past the
end of the file. Where that record ends, and therefore how many
subjects the listing holds, depends on how far a reader chooses to
read, so the run is rejected while reading the listing and no
artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Close the quoting around the comment, and double a quotation mark that
belongs to the text:

    CTX,CTX-02,"Visit missed, rescheduled"
    CTX,CTX-03,"Subject said ""felt fine"""

A comment holding a separator, a quotation mark, or a line break stays
quoted; one holding none of them needs no quoting at all.
