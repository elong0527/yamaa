# ADaM ADSL BMI compute

This fixture derives body mass index with the `compute` expression, which
evaluates one closed numeric formula over current-output columns. It is the
portable counterpart of `../adam-adsl-bmi-function`, which derives the same
value through a project-supplied R function.

```yaml
compute:
  expr: "WEIGHTKG / POWER(NULLIF(HEIGHTCM, 0) / 100, 2)"
```

Grammar, function vocabulary, type promotion, missing-value behavior, and
failure conditions are defined by [R010](../../rules/R010-scalar-computation.md).
The first four rows repeat `../adam-adsl-bmi-function` exactly so the two
derivations can be compared row for row; `CATH-005` adds the zero-height guard.

No value is rounded. R010 has no rounding function, so the derivation carries
full binary64 precision; the four decimal places in the expected output are the
project's float-to-text setting under R011, applied when the value is written.

## What this establishes

1. **A column may appear in every operand position.** `WEIGHTKG` and
   `HEIGHTCM` are both columns and the formula stays one object.

2. **Division and exponentiation are expressible without `function`**, so the
   extension point is not needed for ordinary arithmetic.

3. **Missing propagates without a guarding predicate.** `CATH-004` has no
   height, and `BMI` is missing with no handler declared.

4. **Division by zero is an explicit choice, not a default.** `CATH-005` has a
   collected height of `0`. Under R010 an unguarded division by zero fails the
   run; `NULLIF(HEIGHTCM, 0)` is what converts that record to a missing result.
   A specification that omits the guard is a negative fixture, not this one.

## `CATH-002` is not exactly 25

The stored `BMI` for `CATH-002` is `24.999999999999996`. The value is exact
IEEE 754 arithmetic: `160 / 100` is not representable in binary, so
`POWER(1.6, 2)` is `2.5600000000000005` and the quotient falls one unit in the
last place below `25`. R and Python agree on it.

The expected output records `25` because R011 renders a `float` at the
project's declared four decimal places. The stored value is unchanged and every
comparison, verification, and dependent derivation sees
`24.999999999999996`; only the text is rounded. `../adam-adsl-bmi-function`
records `25` for the same row, and the two fixtures agree because they render
the same value the same way, not because either rounded the arithmetic.

A specification that needs the last places must therefore not read them back
out of the artifact. That is a property of the rendering setting, not of R010.

Written association is significant here. `WEIGHTKG / (HEIGHTCM / 100) / (HEIGHTCM / 100)`
returns exactly `25` for this subject while the `POWER` form does not. Both are
correct; R010 requires an implementation to evaluate the formula as written and
forbids reassociating it.

## Diagnostics and verifications

The key is `[STUDYID, USUBJID]` and exactly six rows are expected. The
`bmi-missing-only-without-usable-height` implication asserts that `BMI` is
missing only for the two records whose height is missing or zero, so a silently
swallowed arithmetic error cannot pass.
