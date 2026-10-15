# Reject Log Zero

**Goal:** carry each collected laboratory result into `AVAL` and
add its natural logarithm as `AVALLN`, with one record for each
subject and parameter.

**Input:** five laboratory records across five subjects: a detectable
viral load (12,000 copies/mL), an undetectable viral load reported as
zero - the record the run rejects - a missing result, a low
near-limit result (40), and a CD4 count (480), showing one record for
each subject and parameter.

**Variables:**

- `AVAL` would be the analysis value, mapped from the collected
  numeric result (`LBSTRESN`).
- `AVALLN` would be the natural logarithm of `AVAL`, which the
  analysis models rather than the untransformed result, and blank
  when `AVAL` is blank. Zero has no logarithm, and a result below the
  limit of detection needs a stated substitution before it can be
  transformed, so the run fails while the values are computed and no
  artifact is accepted.

## How to fix

State the analysis policy for an undetectable value. If it should produce a
missing transformed result, guard zero explicitly:

```yaml
derivation:
  compute:
    expr: "LN(NULLIF(AVAL, 0))"
```

If the study instead substitutes a value related to the assay limit, derive
that stated substitute first and apply `LN` to it. Do not replace zero with an
unstated constant.
