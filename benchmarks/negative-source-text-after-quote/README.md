# Reject Text After a Closing Quote

**Goal:** carry the investigator comment (`CMNT`) for each subject,
one record per subject.

**Input:** collected subject listing holding the investigator
comment (`COMMENT`).

**Variables:**

- `CMNT` would be the comment the investigator recorded about the
  subject, copied from `COMMENT`.

One comment closes its quoting and then carries on with more text before
the next separator. Whether that text belongs to the comment, starts a new
value or is dropped depends on the program that reads the file, so the
comment has no single reading. The run is rejected while the listing is
read, and no artifact is accepted.

## How to fix

Have the program that writes the listing quote the whole comment, so the
closing quotation mark comes right before the separator or the end of the
record:

    STUDYID,USUBJID,COMMENT
    CTX,CTX-01,Dose reduced
    CTX,CTX-02,"Visit missed by site"
    CTX,CTX-03,

If the quotation marks belong to the comment itself, double each one inside
the quoting, as in `"""Visit missed"" by site"`.
