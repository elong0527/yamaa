# Reject Negative Sqrt

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

## How to fix

Confirm and correct the negative weight in the source when it is a data-entry
error. If the analysis explicitly treats a non-positive height or weight as
unusable, encode that policy and return missing instead of evaluating the
square root:

```yaml
derivation:
  case:
    - when: "HEIGHTCM > 0 AND WEIGHTKG > 0"
      then: "SQRT(HEIGHTCM * WEIGHTKG / 3600)"
```

With no `otherwise`, an invalid measurement produces missing.
