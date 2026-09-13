# Reject a subject listing with an unnamed field

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-source-empty-field-name.html)

**Goal:** carry the enrollment site (`SITEID`) for each subject.

**Input:** collected demographics listing carrying a site field
(`SITEID`), whose header ends with a separator that leaves its
last field without a name.

**Variables:**

- `SITEID` would be the site the subject enrolled at, copied from
  the collected listing.

The unnamed field can be asked for by no name, and readers
disagree about whether the field is there at all: one reports
three fields in every record and another four. The run is
rejected and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Name every field the listing carries, or remove the trailing separator when
the header has no further field:

    STUDYID,USUBJID,SITEID

A field worth storing is worth naming, and its name is what every later
reference to it uses.
