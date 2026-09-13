# Reject a treatment ordered but not chosen

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-record-lookup-unordered-keep.html)

**Goal:** derive `TRT01A`, the actual treatment for the first
period, as a single subject-level record from exposure records.

**Input:** demographics records with exposure records carrying
administered treatment (`EXTRT`), start date (`EXSTDTC`), and
sequence number (`EXSEQ`).

**Variables:**

- `TRT01A` would be the treatment from the chosen exposure
  record.

The administrations are placed in start-date and sequence-number
order with no statement of which end supplies the treatment, so a
subject whose administrations disagree has more than one possible
answer. The run is rejected before any data is read and no
artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

State which end of the order supplies the treatment alongside the order
itself. For the earliest treatment:

```yaml
record_lookups:
  - id: DOSING
    dataset: EX
    order_by: [EX.EXSTDTC, EX.EXSEQ]
    keep: first
```

Use the latest end only when the intended result is the latest treatment.
