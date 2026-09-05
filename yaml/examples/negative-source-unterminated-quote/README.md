# ADaM ADSL: reject a comment whose quoted text never closes

This example uses a collected subject listing to attempt one record per
subject:

- `CMNT` is the comment the investigator recorded about the subject.

The last comment opens quoting that it never closes, so the file says its
text continues past the end of the file. Where that record ends, and
therefore how many subjects the listing holds, depends on how far a reader
chooses to read. The run must fail and no artifact is accepted.

## How to fix

Close the quoting around the comment, and double a quotation mark that
belongs to the text:

    CTX,CTX-02,"Visit missed, rescheduled"
    CTX,CTX-03,"Subject said ""felt fine"""

A comment holding a separator, a quotation mark, or a line break stays
quoted; one holding none of them needs no quoting at all.
