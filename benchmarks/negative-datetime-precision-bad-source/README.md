# Reject Datetime Precision from a Partial Date

**Goal:** report whether the first treatment start (`TRTSTPR`) included a
collected time.

**Input:** exposure records carrying a collected start date or date/time.

**Variables:**

- `TRTSTPR`: `S` when the exposure start included a time and `D` when it was a
  complete date with no time. A start collected to the year and month only is
  neither a complete date nor a moment, so it cannot answer whether only the
  time is absent; the run stops rather than reporting a misleading precision.

## How to fix

Recover the exposure day upstream when it was collected. If a date-imputation
policy applies, complete the date separately before asking
`datetime_precision` whether the resulting source carried a time.
