# Reject a presentation order that repeats one value

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-output-order-repeated-term.html)

**Goal:** arrange adverse event (AE) records so each subject's
events read together, newest onset first, carrying the reported
term (`AETERM`) and onset date (`ASTDT`).

**Input:** collected adverse event records carrying the reported
term (`AETERM`) and the onset date (`AESTDTC`).

**Variables:**

- `AETERM` would be the reported term for the event, carried over
  from the collected records.
- `ASTDT` would be the date the event began, carried over from the
  collected onset date.

The records are to be presented by subject ascending, then by
onset date with the most recent first, then by subject descending
again: one value takes two places in the presentation, so there is
no single order the run could give the records. The run is
rejected before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Decide which position the subject holds and state it once. Grouping a
subject's events together while reading the newest first is the subject
ascending first and the date descending:

```yaml
output:
  columns: [STUDYID, USUBJID, ASEQ, AETERM, ASTDT]
  order_by:
    - USUBJID
    - variable: ASTDT
      direction: desc
```

Presenting the subjects in reverse instead means declaring the subject
descending once, in the first position, rather than adding a second entry for
it.
