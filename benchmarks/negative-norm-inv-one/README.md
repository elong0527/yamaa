# Reject a z-score for a percentile of 100

**Goal:** turn each reported BMI-for-age percentile into its z-score, the
standard normal deviate below which that percentage of the population
falls, held in `AVAL`.

**Input:** reported BMI-for-age percentiles with one record per subject and
visit, one of them reported as 100.

**Variables:**

- `PARAMCD`: `BMIZ`, the BMI-for-age z-score.
- `AVAL` would be the z-score of the reported percentile, and blank when
  the percentile is blank. A percentile of 100, like one of 0, has no
  finite z-score, so the run fails while the values are computed and no
  artifact is accepted.

## How to fix

A percentile reported as 100 or 0 is usually a rounded one: the z-score it
came from was finite, and the rounding lost it. Prefer deriving the z-score
from the measurement and its growth reference, as the percentile itself was
derived. If only the percentile is available, state that a percentile at
either bound gives no z-score:

```yaml
derivation:
  case:
    - when: "PCTL > 0 AND PCTL < 100"
      then: "NORM_INV(PCTL / 100)"
    - otherwise: null
```

Do not move a bound to an unstated nearby percentile.
