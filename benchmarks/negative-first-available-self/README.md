# Reject Self Fallback

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-first-available-self.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** fill `SEVAL`, the severity recorded at the visit, from
the collected severity when present, otherwise from `SEVAL`
itself, defaulting to `0` when neither gives a value.

**Input:** vital-sign records carrying study, subject, and sequence
entries plus the collected severity (`SEVAL`), which is missing
when no severity was recorded.

**Variables:**

- `SEVAL`: the collected severity when one was recorded at the
  visit, otherwise the value of `SEVAL` itself, otherwise `0`.
  Because `SEVAL` reads its own value, there is no earlier value to
  carry and the definition loops back on itself, so the run is
  rejected before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADVS

## How to fix

Carry forward from the collected source instead of the column being
derived. Search the subject's earlier collected values in a separate column,
and fall back to it, then to `0`, only when the current record has no
severity:

```yaml
- name: SEVPREV
  type: int
  derivation:
    previous_non_missing:
      source: VS.SEVAL
      window:
        group_by: [STUDYID, USUBJID]
        order_by: [VSSEQ]

- name: SEVAL
  type: int
  derivation:
    first_available:
      sources: [VS.SEVAL, SEVPREV]
      missing: 0
```

Keep `SEVPREV` internal by omitting it from `output.columns`. A subject whose
first record has no severity gets `0` there, because no earlier value exists.
