# Reject a body surface area from a negative weight

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-compute-sqrt-of-negative.html)

**Goal:** derive body surface area (`BSA`) for each subject from
collected height and weight.

**Input:** demographic records carrying collected height
(`HEIGHTCM`) and weight (`WEIGHTKG`).

**Variables:**

- `BSA` would contain body surface area in square meters, the
  square root of the collected height (`HEIGHTCM`) times the
  collected weight (`WEIGHTKG`) divided by `3600`.

A negative weight leaves the area with no real value. Any answer
would be invented rather than derived, so the run fails and no
artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Confirm and correct the negative weight in the source when it is a data-entry
error. If the analysis explicitly treats a non-positive height or weight as
unusable, encode that policy and return missing instead of evaluating the
square root:

```yaml
derivation:
  case:
    branches:
      - when: "HEIGHTCM > 0 AND WEIGHTKG > 0"
        then:
          compute:
            expr: "SQRT(HEIGHTCM * WEIGHTKG / 3600)"
```

With no `otherwise`, an invalid measurement produces missing.
