# Reject a Site-Scoped Subject Identity

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-keys-internal.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the investigator (`INVID`) responsible for each
subject's site.

**Input:** collected demographics carrying the study site (`SITEID`)
and its investigator (`INVID`).

**Variables:**

- `INVID` would be the investigator of the subject's site, read
  from `INVID`.

**Note:** the record identity includes the site (`SITEID`), but the
result does not carry it, so a reader of the result could not check
which site a row answers for. The run is rejected before any data is
read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Every column that forms the record identity must appear in the
output. If the site is part of the identity, carry it:

```yaml
output:
  columns: [STUDYID, USUBJID, SITEID, INVID]
```

If study and subject already form the intended identity, drop the
site from the identity instead:

```yaml
keys: [STUDYID, USUBJID]
```

Choose the option that matches the output's actual row identity.
