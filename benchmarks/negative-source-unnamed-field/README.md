# Reject Unnamed Field

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-source-unnamed-field.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the enrollment site (`SITEID`) for each subject.

**Input:** collected demographics listing carrying a site field
(`SITEID`), whose records all end with a separator, which leaves the
header's last field without a name.

**Variables:**

- `SITEID` would be the site the subject enrolled at, copied from
  the collected listing.

The unnamed field can be asked for by no name, and readers
disagree about whether the field is there at all: one reports
three fields in every record and another four. The run is
rejected and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Name every field the listing carries, or, when the listing has no further
field, remove the trailing separator from the header and from every record.
Removing it from the header alone leaves each record one field longer than
the header, which is rejected too:

    STUDYID,USUBJID,SITEID
    CTX,CTX-01,701
    CTX,CTX-02,702

A field worth storing is worth naming, and its name is what every later
reference to it uses.
