# Reject Formula Flag

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-formula-flag.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `AVAL`, `ANRHI`, and `HIFL` for each subject and
parameter, flagging collected laboratory results that lie above
their reference range.

**Input:** collected laboratory records carrying the study and
subject identifiers with `LBTESTCD`, the test code; `LBSTRESN`,
the numeric result; and `LBSTNRHI`, the upper limit of its
reference range.

**Variables:**

- `AVAL` would be the collected result, taken from `LBSTRESN`.
- `ANRHI` would be the upper limit of the reference range, taken
  from `LBSTNRHI`.
- `HIFL` would be `1` for a result above that limit and `0`
  otherwise.

**Note:** the flag is written as the comparison `AVAL > ANRHI`
inside a formula. A formula produces a number, while a comparison
answers yes or no; turning that answer into `1` or `0`, or into
text, would each be a different result from the same comparison,
so the run is rejected before any data is read and no artifact
is accepted. `HIFL` is this benchmark's own invention, documented
here; `AVAL` and `ANRHI` follow ADaMIG-1.3.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Ask the comparison where the flag is decided, so the conversion
from a yes-or-no answer to `1` or `0` is explicit:

```yaml
- name: HIFL
  type: int
  derivation:
    case:
      - when: "AVAL > ANRHI"
        then: {literal: 1}
      - otherwise: {literal: 0}
```

A missing result or limit answers neither yes nor no, so it falls
to `otherwise` and gets `0`; to leave such a flag empty instead,
replace the `otherwise` with `when: "AVAL <= ANRHI"` returning `0`.
