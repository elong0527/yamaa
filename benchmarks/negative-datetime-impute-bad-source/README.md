# Reject Partial Date as a Datetime Source

**Goal:** derive the first treatment date/time (`TRTSDTM`) from collected
exposure start text.

**Input:** exposure records carrying a collected start date or date/time.

**Variables:**

- `TRTSDTM`: the collected exposure start date/time, or the first second of a
  completely collected date when only its time is absent. A start collected to
  the year and month only names no day, and supplying a time cannot supply
  one, so the run stops rather than inventing both a day and a time.

## How to fix

Recover the exposure day upstream when it was collected. If the analysis plan
also defines a date-imputation policy, complete the date in a separate named
column before applying `datetime_impute`; do not silently combine two policies.
