# ADaM ADSL: reject a subject listing with an unnamed field

This example uses a collected subject listing to attempt one record per
subject:

- `SITEID` is the site the subject enrolled at.

The listing's header ends with a separator, so its last field has no name.
Nothing can ask that field for its values, and readers disagree about whether
the field is there at all: one reports three fields in every record and
another four. The run must fail and no artifact is accepted.

## How to fix

Name every field the listing carries, or remove the trailing separator when
the header has no further field:

    STUDYID,USUBJID,SITEID

A field worth storing is worth naming, and its name is what every later
reference to it uses.
