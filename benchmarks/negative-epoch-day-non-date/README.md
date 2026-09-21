# Epoch Day Non-Date

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-epoch-day-non-date.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** fix the failure behavior of `to_epoch_day` when the source
is not a date.

**Input:** exposure records with a text start date (`EXSTDT` as str,
not date).

**Expected:** validation fails with `incompatible_input_type`
because `to_epoch_day` requires a `date` input. A text value must
first be converted via `to_date`.

## How to fix

Convert the text to a date before calling `to_epoch_day`:

```yaml
- name: EPOCHDAY
  type: int
  derivation:
    to_epoch_day:
      source:
        to_date:
          source: EX.EXSTDT
```

The allowed column types are `str`, `int`, `float`, `date`, and
`datetime`.
