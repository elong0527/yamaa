# Reject an Imputation Flag Read From Collected Text

**Goal:** one row per adverse event (AE) with an analysis start date
(`ASTDT`) completed from the collected start, and a flag (`ASTDTF`) marking
how much of that date was supplied.

**Input:** adverse event records carrying the collected start (`AESTDTC`): a
year and month, or a full date.

**Variables:**

- `ASTDT` would be the analysis start date: a full date as collected, and a
  year and month completed to the first of the month.
- `ASTDTF` would be `D` when the day of `ASTDT` was supplied. The flag
  describes a completed date, and collected text has not been completed, so
  the run stops before reading any data.

## How to fix

Read the flag from the completed date, which carries how much of it was
collected:

```yaml
imputation_flag:
  source: ASTDT
  part: date
```
