# Reject Internal Key

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-keys-internal.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `INVID` for each subject.

**Input:** collected demographics carrying study site (`SITEID`)
and investigator (`INVID`).

**Variables:**

- `INVID` would be the investigator responsible for the subject's
  site, read from `INVID`.

**Note:** the record identity depends on a site value the result does
not carry, so a reader of the result could not check it. The run is
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
`SITEID` from the record identity instead:

```yaml
keys: [STUDYID, USUBJID]
```

Choose the option that matches the output's actual row identity.
