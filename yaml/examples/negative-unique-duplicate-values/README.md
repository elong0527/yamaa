# Reject a SITEID uniqueness check when one site enrolls more than one subject

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-unique-duplicate-values.html)

**Goal:** build the analysis dataset with one record per subject
carrying `SITEID`.

**Input:** collected demographics records with `SITEID` giving the
enrolling site.

**Variables:**

- `SITEID` would hold the site the subject enrolled at, copied from
  the collected site.

The completed-dataset check rejects the run with no artifact
accepted because one `SITEID` value is shared by more than one
subject and so cannot uniquely identify a subject record.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide whether a record represents a subject or a site before editing
anything. Assert uniqueness on the columns that truly identify a record.
Either check the subject identifiers:

```yaml
verifications:
  - unique:
      columns: [STUDYID, USUBJID]
```

or, when one record per site is the intent, aggregate the subjects to site
records before asserting.
