# Reject the base-10 logarithm of a zero titer

**Goal:** carry each neutralizing antibody titer into `AVAL` on the base-10
logarithmic scale, with one record for each subject, parameter and visit.

**Input:** immunogenicity results with one neutralizing antibody titer per
subject and visit and the assay's lower limit of quantitation, one titer
reported as zero because no dilution neutralized the virus.

**Variables:**

- `PARAMCD`: `NABLOG10`, the base-10 logarithm of the neutralizing antibody
  titer.
- `AVAL` would be the base-10 logarithm of the titer, and blank when the
  titer is blank. A titer of zero has no logarithm, so the run fails while
  the values are computed and no artifact is accepted.

## How to fix

Decide what a titer below the lower limit of quantitation means for the
analysis. A log-scale analysis usually leaves such titers out; choose the
records with a filter that keeps the quantified and the blank titers:

```yaml
filter: "IS.ISSTRESN IS NULL OR IS.ISSTRESN >= IS.ISLLOQ"
```

If the study instead imputes such titers, for example as half the lower
limit, derive that stated value first and take its logarithm. Do not
replace zero with an unstated constant.
