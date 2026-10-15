# Reject Trivial Filter

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-source-trivial-filter.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the planned arm (ARM) and the actual arm (ACTARM) for each
subject.

**Input:** EDC output in long form, one row per collected item.

**Variables:**

- **ARM** would be the planned arm as collected; `Unassigned` when none
  was collected.
- **ACTARM** would be the actual arm, a copy of the planned arm, but the
  copy also states which collected rows to choose among. The planned arm
  is one value by then rather than a set of rows, so the run is rejected
  before any data is read and no artifact is accepted.

**Standard:** SDTM | **Domain:** DM

## How to fix

Decide whether the actual arm repeats a value the record already carries
or reads the collected rows itself. Repeating the planned arm names it and
nothing else:

```yaml
- name: ACTARM
  type: str
  label: Description of Actual Arm
  derivation:
    source: ARM
```

When the actual arm is collected in its own right, read the rows that hold
it, exactly as the planned arm does:

```yaml
- name: ACTARM
  type: str
  label: Description of Actual Arm
  derivation:
    first_available:
      sources:
        - filter: ODM.ItemOID = 'IT.DM.ACTARM'
          variable: ODM.Value
      missing: Unassigned
```
