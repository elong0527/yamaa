# Reject a site-scoped subject identity

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-keys-internal-column.html)

**Goal:** derive `INVID` for each subject.

**Input:** collected demographics carrying input site (`SITEID`)
and investigator (`INVID`).

**Variables:**

- `INVID` would be the investigator responsible for the subject
  site, read from `INVID`.

The record identity depends on a site value the result does not
carry, so a reader of the result could not check it. The run is
rejected before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Every key column must be present in the output. If `SITEID` is part of the
record identity, add it to the output columns:

```yaml
output:
  columns: [STUDYID, USUBJID, SITEID, INVID]
```

If study and subject already form the intended unique identity, remove
`SITEID` from the record identity instead. Choose the option that matches the
output's actual grain.
