# Window on a Window-Derived Value

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-row-window-on-window-result.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** attempt the previous albumin result (`PREV_AVAL`), the change
from it (`CHG`), and the previous visit's change (`PREV2`), where the
last reads a neighboring visit's value that was itself computed from a
neighboring visit.

**Input:** laboratory (LB) records carrying a test code (`LBTESTCD`),
a numeric result (`LBSTRESN`), and a visit number (`VISITNUM`).

**Variables:**

- `PREV_AVAL` would be the subject's albumin result (`AVAL`) at the
  previous visit.
- `CHG` would be `AVAL - PREV_AVAL`, the change since that visit.
- `PREV2` would be `CHG` at the previous visit.

Every value that reads a neighboring visit is computed in one pass over
the rows being built, with no stated order among those values. `PREV2`
reads a neighboring visit's `CHG`, which depends on `PREV_AVAL`, so it
would read values whose computation order is unspecified, and the run is
rejected before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADLBC

## How to fix

Decide whether the previous visit's change is needed. If it is not, drop
`PREV2`: `CHG` already gives each visit's change.

If it is, have every window read the result `AVAL` itself, never a value
computed from another window, and subtract afterwards: the previous
visit's change is the previous result minus the result two visits back.

```yaml
derivations:
  # USUBJID, PARAMCD, AVISITN, AVAL, PREV_AVAL and CHG as before
  PREV2_AVAL:
    row_value:
      source: AVAL
      offset: -2
      window:
        group_by: [USUBJID, PARAMCD]
        order_by: [AVISITN]
  PREV2:
    compute:
      expr: "PREV_AVAL - PREV2_AVAL"
```

Declare `PREV2_AVAL` as a float column and leave it out of the output.
