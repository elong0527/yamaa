# Reject Negative Sqrt

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-negative-sqrt.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive body surface area (`BSA`) for each subject from
collected height and weight.

**Input:** demographic records carrying collected height
(`HEIGHTCM`) and weight (`WEIGHTKG`).

**Variables:**

- `BSA` would contain body surface area in square meters by the
  Mosteller formula: height in centimeters (`HEIGHTCM`) times
  weight in kilograms (`WEIGHTKG`), divided by `3600`, then
  square-rooted; missing when either measurement is missing.

**Note:** when height times weight is negative, as with a negative
weight, the area has no real value. Any answer would be invented
rather than derived, so the run fails and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Confirm and correct the negative weight in the source when it is a data-entry
error. If the analysis explicitly treats a non-positive height or weight as
unusable, encode that policy and return missing instead of evaluating the
square root:

```yaml
derivation:
  case:
    - when: "HEIGHTCM > 0 AND WEIGHTKG > 0"
      then:
        compute:
          expr: "SQRT(HEIGHTCM * WEIGHTKG / 3600)"
```

With no `otherwise`, an invalid measurement produces missing.
