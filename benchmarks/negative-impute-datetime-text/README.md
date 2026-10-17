# Reject a Date and Time Written With a Space

**Goal:** derive the first treatment date/time (`TRTSDTM`) from collected
exposure start text.

**Input:** exposure records carrying a collected start: a year and month, a
date and time, or a date and time written with a space instead of `T`.

**Variables:**

- `TRTSDTM`: the collected exposure start date/time, with any part of the
  time that was not collected taking its first value. A start collected to
  the year and month only gives no date/time, because no day is supplied. A
  start written with a space before its time is not ISO 8601 date or
  date/time text, so the run stops rather than guessing which part is the
  time.

## How to fix

Correct the export upstream so the start uses ISO 8601 text, with `T`
between the date and the time, such as `2025-01-15T09:30`. If the study
decides that such starts carry no usable date/time, state that decision
explicitly:

```yaml
impute:
  type: datetime
  source: EX.EXSTDTC
  minimum_source_precision: day
  time: first
  invalid: null
```
