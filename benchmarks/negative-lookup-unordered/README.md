# Reject Unordered Choice

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-lookup-unordered.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `TRT01A`, the actual treatment for the first
period, as a single subject-level record from exposure records.

**Input:** demographics records with exposure records carrying
administered treatment (`EXTRT`), start date (`EXSTDTC`), and
sequence number (`EXSEQ`).

**Variables:**

- `TRT01A` would be the treatment from the chosen exposure
  record.

**Note:** the administrations are placed in start-date and
sequence-number order with no statement of which end supplies the
treatment, so a subject whose administrations disagree has more
than one possible answer. The run is rejected before any data is
read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

State which end of the order supplies the treatment alongside the order
itself. For the earliest treatment:

```yaml
intermediates:
  - id: DOSING
    dataset: EX
    order_by: [EX.EXSTDTC, EX.EXSEQ]
    keep: first
```

Use the latest end only when the intended result is the latest treatment.
