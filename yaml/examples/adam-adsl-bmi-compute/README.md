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

No value is rounded. R010 requires a derivation to carry full precision and
leaves the number of places shown to the report, so this fixture does not use
`ROUND` and no fixture in the suite does.

## What this establishes

1. **A column may appear in every operand position.** `add.addend` and
   `multiply.factor` are literals and `subtract` types both operands as
   variables, so before `compute` the schema could subtract two columns but
   could not add, divide, or exponentiate them. Here `WEIGHTKG` and `HEIGHTCM`
   are both columns and the formula stays one object.

2. **Division and exponentiation are expressible without `function`.**
   `../adam-adsl-bmi-function` exists because `divide` is unregistered. That
   fixture now establishes only the runtime-function call lifecycle; BMI itself
   no longer needs the extension point.

3. **Missing propagates without a guarding predicate.** `CATH-004` has no
   height, and `BMI` is missing with no handler declared. R007 leaves the
   missing-value behavior of `multiply`, `add`, and `subtract` undefined, so
   specifications using them must guard with an explicit `IS NOT NULL`
   predicate, as `../sdtm-vs-unit-standardization` does. R010 fixes the answer
   for `compute`: `NULL` propagates.

4. **Division by zero is an explicit choice, not a default.** `CATH-005` has a
   collected height of `0`. Under R010 an unguarded division by zero fails the
   run; `NULLIF(HEIGHTCM, 0)` is what converts that record to a missing result.
   A specification that omits the guard is a negative fixture, not this one.

## `CATH-002` and float-to-string conversion

`BMI` for `CATH-002` is `24.999999999999996`, not `25`. The value is exact
IEEE 754 arithmetic: `160 / 100` is not representable in binary, so
`POWER(1.6, 2)` is `2.5600000000000005` and the quotient falls one unit in the
last place below `25`. R and Python agree on this value.

`../adam-adsl-bmi-function` computes the same formula in R and its expected
output records `25`. **That value is not the full-precision result.** This
fixture commits the exact value instead, following the shortest-round-trip
proposal in `../sdtm-vs-unit-standardization`. The discrepancy is evidence for
the unresolved float-to-string question in R005, not an R010 question: no
rounding mode, association, or grammar decision changes it.

It follows that comparing a golden output by string equality requires that rule
to be settled. Rounding is not the escape: under R010 the dataset carries
`24.999999999999996` and the report decides how it is shown.

The same question is open in `../adam-adlb-bds`, whose expected `AVAL` of
`0.167` for the ALTSI parameter is the shortened form of `0.16699999999999998`,
and in `../adam-adsl-bmi-function`.

Written association is significant here. `WEIGHTKG / (HEIGHTCM / 100) / (HEIGHTCM / 100)`
returns exactly `25` for this subject while the `POWER` form does not. Both are
correct; R010 requires an implementation to evaluate the formula as written and
forbids reassociating it.

## Diagnostics and verifications

The key is `[STUDYID, USUBJID]` and exactly six rows are expected. The
`bmi-missing-only-without-usable-height` implication asserts that `BMI` is
missing only for the two records whose height is missing or zero, so a silently
swallowed arithmetic error cannot pass.
