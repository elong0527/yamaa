# Reject Nested Expression

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-variable-nested.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry `COUNTRY` on one record per subject, holding the
subject's collected country in capitals, or the site country when
the subject's own entry is blank.

**Input:** collected demographics records carrying the collected
country (`COUNTRY`) and the site country (`SITECNTY`).

**Variables:**

- `COUNTRY` would be the subject's own country in capitals, or the
  site country in capitals when the subject's own entry is blank.

**Note:** the uppercasing step is also asked to choose between the
two entries, but it can only read one named value. Choosing the
entry is a step of its own whose result must be named first, so the
run is rejected before any data is read and no artifact is
accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Derive the selected source in a helper column, then pass that value to the
uppercasing step:

```yaml
- name: COUNTRYSRC
  type: str
  derivation:
    first_available:
      sources: [DM.COUNTRY, DM.SITECNTY]

- name: COUNTRY
  type: str
  derivation:
    str_upper:
      source: COUNTRYSRC
```

Keep the helper column out of the artifact by leaving it off the artifact
column list. A subject with both entries blank then stops the run; if that can
happen, add `missing: null` beside `source: COUNTRYSRC` to leave that
subject's country blank instead.
