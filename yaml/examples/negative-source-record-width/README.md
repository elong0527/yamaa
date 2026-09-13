# Reject a record with an extra field

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-source-record-width.html)

**Goal:** build a subject-level result carrying the enrollment
site (`SITEID`) and the recorded sex (`SEX`) for each subject.

**Input:** subject listing records holding the enrollment site
(`SITEID`) and the recorded sex (`SEX`). One record carries one
more field than the header names, and nothing says what the
surplus value holds.

**Variables:**

- `SITEID` would be the site the subject enrolled at, taken from
  the collected listing.
- `SEX` would be the recorded sex, taken from the collected
  listing.

The result would carry `SITEID` and `SEX` beside the study and
subject identifiers, but no row is produced: one record carries a
field more than the listing names, so reading it would mean either
dropping a collected value or shifting every value after it into
the wrong field, and the run is rejected while reading the listing
with no artifact accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Export one field for each named field in every record. Where the extra value
is itself collected, name it in the header so that every record carries it:

    STUDYID,USUBJID,SITEID,SEX,SITENM

A bare separator inside a value produces the same surplus, so a value holding
a separator is quoted rather than left bare.
