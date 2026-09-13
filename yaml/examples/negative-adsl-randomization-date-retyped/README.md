# Reject a randomization date described twice

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-adsl-randomization-date-retyped.html)

**Goal:** carry the subject randomization date `RANDDT` from
demographics into the subject-level analysis file.

**Input:** a subject-level demographics file holding the study and
subject identifiers with `RANDDT`, produced from an Operational
Data Model (ODM) extract.

**Variables:**

- `RANDDT` would be the subject's randomization date, taken from
  `RANDDT` in the demographics file, but no row is produced.

The date's value kind is already fixed where the demographics
file is produced. Stating it again beside the source would leave
two authorities for the same field even when they agree, so the
run is rejected before any data is read and no artifact is
accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Keep the producing DM specification as the single type authority and remove
the inline `types` entry:

```yaml
datasets:
  DM:
    path: input/dm.csv
    schema: input/dm.schema.yaml
```
