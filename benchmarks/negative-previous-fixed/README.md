# Reject Fixed Carryforward

**Goal:** carry the systolic blood pressure (`SYSBP`) forward so
the prior systolic pressure (`PRIORBP`) holds the closest earlier
taken pressure for the same subject.

**Input:** collected vital signs records, each holding a systolic
blood pressure (`SYSBP`); the value is missing when the
measurement was not taken.

**Variables:**

- `PRIORBP` would hold the closest earlier taken pressure for the
  same subject, in sequence-number order, and be missing when no
  earlier pressure was taken.

**Note:** the carried value arrives as the fixed number `120`
instead of a variable to search, so the run is rejected before any
data is read and no artifact is accepted.

## How to fix

Decide which earlier measurement the study carries, then name the variable
that holds it. When the closest earlier taken pressure is the rule, search the
pressure variable itself:

```yaml
- name: PRIORBP
  type: int
  derivation:
    previous_non_missing:
      source: SYSBP
      window:
        group_by: [STUDYID, USUBJID]
        order_by: [VSSEQ]
```

When every gap should instead read a fixed fallback, state that as a separate
rule over the carried result rather than inside the search.
