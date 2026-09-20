# Reject a selection over a value that is already one value

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-source-filter-without-records.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** carry the planned arm (ARM) and the actual arm (ACTARM) for each
subject.

**Input:** EDC output in long form, one row per collected item.

**Variables:**

- **ARM**: planned arm as collected; `Unassigned` when none was collected.
- **ACTARM** would be the actual arm, always the planned arm in this
  example, but the copy also states which collected rows to choose among.
  The planned arm is one value by then rather than a set of rows, so the
  run is rejected before any data is read and no artifact is accepted.

**Standard:** SDTM | **Domain:** DM

## How to fix

Decide whether the actual arm repeats a value the record already carries
or reads the collected rows itself. Repeating the planned arm names it and
nothing else:

```yaml
- name: ACTARM
  type: str
  derivation:
    source: ARM
```

When the actual arm is collected in its own right, read the rows that hold
it, exactly as the planned arm does:

```yaml
- name: ACTARM
  type: str
  derivation:
    first_available:
      sources:
        - filter: ODM.ItemOID = 'IT.DM.ACTARM'
          variable: ODM.Value
      default: Unassigned
```
