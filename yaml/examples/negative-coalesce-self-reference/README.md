# Reject a severity that falls back to its own column

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-coalesce-self-reference.html)

**Goal:** fill `SEVAL`, the severity recorded at the visit, from
the collected severity when present, otherwise from `SEVAL`
itself, defaulting to `0` when neither gives a value.

**Input:** vital-sign records carrying study, subject, and sequence
entries plus the collected severity (`SEVAL`), which is missing
when no severity was recorded.

**Variables:**

- `SEVAL`: the collected severity when one was recorded at the
  visit, otherwise the value of `SEVAL` itself, otherwise `0`.

`SEVAL` reads its own value, so there is no earlier value to carry
and the definition loops back on itself. The run is rejected
before any data is read, and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADVS

## How to fix

Carry forward from the collected source instead of the column being
derived. Search the collected values on the earlier rows:

```yaml
previous_non_missing:
  source: VS.SEVAL
```

and, when the first row may itself be missing, keep the default for that
row only.
