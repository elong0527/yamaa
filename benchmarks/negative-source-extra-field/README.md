# Reject Extra Field

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-source-extra-field.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

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

Reading the longer record would mean either dropping a collected
value or shifting every value after it into the wrong field, so the
run is rejected while reading the listing and no artifact is
accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Export one field for each named field in every record. Where the extra value
is itself collected, name it in the header and give every record the field,
left empty where nothing was collected:

    STUDYID,USUBJID,SITEID,SEX,SITENM
    CTX,CTX-01,701,F,
    CTX,CTX-02,702,M,Royal Infirmary

A bare separator inside a value produces the same surplus, so a value holding
a separator is quoted rather than left bare.
