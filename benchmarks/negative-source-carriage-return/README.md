# Reject a Carriage Return Inside a Field

**Goal:** carry the site name (`SITENM`) onto a subject listing.

**Input:** collected demographics carrying `SITENM`, exported from a
spreadsheet in which one site name was typed on two lines of its cell.

**Variables:**

- `SITENM` would be the name of the site the subject enrolled at, taken from
  the collected value.

The program that wrote the file broke that name with a lone carriage return,
the line break of some older programs, instead of the line feed that breaks a
line everywhere else in the file. A carriage return belongs only at the end
of a record, just before its line feed, so whether the name ends there,
carries on, or starts a new record depends on the program that reads the
file. The run is rejected while the file is read, and no artifact is
accepted.

## How to fix

Decide what the site's name is and write it on one line at the source, then
export the listing again rather than editing the file by hand:

    STUDYID,USUBJID,SITENM
    CTX,CTX-31,Rigshospitalet
    CTX,CTX-32,"Royal Infirmary, Edinburgh"
    CTX,CTX-33,Karolinska

A name that must keep its line break is quoted and broken with a line feed
alone, which the reader keeps as part of the value.
