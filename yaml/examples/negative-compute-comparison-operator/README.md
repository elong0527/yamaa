# Reject an above-range flag written as a formula

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-compute-comparison-operator.html)

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
- `HIFL` would mark a result above that limit by answering
  whether `AVAL` exceeds `ANRHI`.

A formula produces a number, and a comparison answers yes or no.
Turning that answer into `1` or `0`, or into text, would each be
a different result from the same specification, so the run is
rejected before any data is read and no artifact is accepted. A
comparison belongs where the specification asks a question rather
than calculates a value.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Use `case` to ask the comparison and return the intended numeric flag:

```yaml
- name: HIFL
  type: int
  derivation:
    case:
      branches:
        - when: "AVAL > ANRHI"
          then: {literal: 1}
      otherwise: {literal: 0}
```

This makes the conversion from a yes-or-no answer to `1` or `0` explicit.
