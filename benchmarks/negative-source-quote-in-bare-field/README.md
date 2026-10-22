# Reject a Quotation Mark in an Unquoted Comment

**Goal:** carry the investigator comment (`CMNT`) for each subject,
one record per subject.

**Input:** collected subject listing holding the investigator
comment (`COMMENT`).

**Variables:**

- `CMNT` would be the comment the investigator recorded about the
  subject, copied from `COMMENT`.

One comment quotes the subject's own words, but the comment itself is not
quoted, so its quotation marks sit inside unquoted text. A reader that
treats them as text keeps the marks, and one that treats them as quoting
drops them or reads on into the next record, so what the comment says
depends on the reader. The run is rejected while the listing is read, and
no artifact is accepted.

## How to fix

Have the program that writes the listing quote every comment that holds a
quotation mark, doubling each mark inside the quoting:

    STUDYID,USUBJID,COMMENT
    CTX,CTX-01,Dose reduced
    CTX,CTX-02,"Subject said ""felt fine"""
    CTX,CTX-03,

A comment without a separator, quotation mark or line break needs no
quoting at all.
