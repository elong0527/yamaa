# ADaM ADSL: reject a body surface area from a negative weight

This example uses collected height and weight to attempt one record per
subject:

- `BSA` is body surface area in square metres, derived from the collected
  height and weight.

One weight carries a leading minus sign, so the area has no real value. Any
answer would be invented rather than derived, so the run must fail and no
artifact is accepted.

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
