# Reject extracting a date from a date

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-to-date-date-source.html)

**Goal:** one analysis row for each adverse event carrying the
analysis start (`ASTDT`) and a second analysis start (`ASTDT2`).

**Input:** collected adverse event records carrying the reported
start (`AESTDTC`).

**Variables:**

- `ASTDT` would contain the analysis start date taken from the
  reported start.
- `ASTDT2` would be the calendar date taken from the analysis
  start, but taking a calendar date needs a local datetime while
  the analysis start is already a date, so the run is rejected
  before any data is read and no row is produced.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Use the date directly. When the source is a datetime, extract its calendar
date explicitly:

```yaml
- name: ASTDT2
  type: date
  derivation:
    to_date: {source: ASTDTM}
```
