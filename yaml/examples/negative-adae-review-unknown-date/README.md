# Reject a review flag that names an unavailable date

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-adae-review-unknown-date.html)

**Goal:** derive `REVIEWFL` to flag adverse events (AEs) for
protocol review.

**Input:** collected adverse event records carrying the collected
onset date (`AESTDTC`) alongside the study, subject, and event
sequence identifiers.

**Variables:**

- `REVIEWFL`: would contain `Y` when the named date is on or
  after `DATE '2025-01-01'` and `N` otherwise; the decision names
  `UNKNOWNDT`, which no declared column provides, so the
  specification is rejected before any data is read.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Use the declared analysis start date in the condition:

```yaml
when: "ASTDT >= DATE '2025-01-01'"
```
