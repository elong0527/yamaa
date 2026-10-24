# Reject a geometric mean over a zero ratio

**Goal:** carry on each subject-level record `MRGM`, the geometric mean of
the subject's metabolite-to-parent ratios of maximum concentration across
periods.

**Input:** demographics with one record per subject, and the
pharmacokinetic parameters with one metabolite ratio per subject and
period, one of them zero because the metabolite concentration was below the
limit of quantification.

**Variables:**

- `MRGM` would be the exponential of the mean of the natural logarithms of
  the subject's ratios. A ratio of zero has no logarithm, so no geometric
  mean exists for that subject, and the run fails while the values are
  computed and no artifact is accepted.

## How to fix

Decide what a ratio of zero means for the analysis. The usual geometric
mean is taken over the ratios above zero, which leaves the zero ratio out:

```yaml
- name: MRGM
  type: float
  derivation:
    aggregate:
      key: [STUDYID, USUBJID]
      filter: "PP.PPTESTCD = 'MRCMAX' AND PP.PPSTRESN > 0"
      expr: "GEOMEAN(PP.PPSTRESN)"
```

If the study instead substitutes a value related to the limit of
quantification, derive that stated substitute first and take the geometric
mean of it. Do not replace zero with an unstated constant.
