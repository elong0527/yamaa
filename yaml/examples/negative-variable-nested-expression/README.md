# Reject an uppercased country chosen inside the same step

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-variable-nested-expression.html)

**Goal:** carry `COUNTRY` on one record per subject, holding the
subject's collected country in capitals, or the site country when
the subject's own entry is blank.

**Input:** collected demographics records carrying the collected
country (`COUNTRY`) and the site country (`SITECNTY`).

**Variables:**

- `COUNTRY` would be the subject's own country in capitals, or the
  site country in capitals when the subject's own entry is blank,
  but no row is produced.

The uppercasing step also picks which text to uppercase, so the
pick sits inside another operation and no named value feeds the
step. Each result is built one step at a time from a named value,
so the run is rejected before any data is read and no artifact is
accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Derive the selected source in a helper column, then pass that value to the
uppercasing step:

```yaml
- name: COUNTRYSRC
  type: str
  derivation:
    coalesce:
      sources: [DM.COUNTRY, DM.SITECNTY]

- name: COUNTRY
  type: str
  derivation:
    str_upper:
      source: COUNTRYSRC
```

Keep the helper column out of the artifact by leaving it off the artifact
column list.
