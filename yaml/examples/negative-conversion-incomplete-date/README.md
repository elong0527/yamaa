# Reject an event start date that names no day

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-conversion-incomplete-date.html)

**Goal:** carry the collected adverse event (AE) start text
(`AESTDTC`) into `ASTDT`.

**Input:** adverse event records, each identified by study,
subject, and sequence, carrying reported term (`AETERM`) and
start text (`AESTDTC`).

**Variables:**

- `ASTDT`: the event start date, which would carry the collected
  start text (`AESTDTC`). The collected text `2023-06` names a
  month with no day, and a date answers with one day, so
  recording the first, the last, or the middle day would each
  answer with a day nobody recorded. The text cannot be read as a
  date, so the run fails and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Declare how a partial date is completed. For example, to use the earliest date
the collected text permits:

```yaml
derivation:
  date_impute:
    source: AE.AESTDTC
    month: 1
    day: 1
```

A complete source date is retained; `2023-06` becomes `2023-06-01`.
