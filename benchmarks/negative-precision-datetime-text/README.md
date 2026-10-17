# Reject Datetime Precision of a Zoned Date and Time

**Goal:** report how much of the first treatment start (`TRTSTPR`) was
collected.

**Input:** exposure records carrying a collected start: a year and month, a
date and time to the second, or a date and time carrying a time zone.

**Variables:**

- `TRTSTPR`: `month` for a start collected to the year and month, and
  `second` for a start collected to the second; a start collected to the
  day, hour or minute gives `day`, `hour` or `minute`. A start carrying a
  time zone is not a local date and time, so the run stops rather than
  reporting a precision for it.

## How to fix

Correct the export upstream so the start is local ISO 8601 text without a
zone, such as `2025-01-15T09:30:00`, after any conversion the study defines.
If the study decides that such starts have no usable precision, state that
decision explicitly:

```yaml
precision:
  type: datetime
  source: EX.EXSTDTC
  invalid: null
```
