# Reject a treatment and dose taken from an unchosen record

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-record-lookup-unordered-choice.html)

**Goal:** carry the treatment a subject received and the dose they
received of it onto `TRT01A` and `TRT01DOSE`.

**Input:** demographics records for each subject, with exposure
records carrying treatment (`EXTRT`), dose (`EXDOSE`), sequence
number (`EXSEQ`), and start date (`EXSTDTC`).

**Variables:**

- `TRT01A` would contain the treatment from the subject's chosen
  exposure record.
- `TRT01DOSE` would contain the dose from that same record.

**Note:** the pair must come from one administration, but nothing
says which one, so a subject matching more than one exposure
record has no single answer, and the run is rejected with no
artifact accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

State the full order and which end of that order to keep. To choose the
earliest administration:

```yaml
record_lookups:
  - id: DOSING
    dataset: EX
    order_by: [EX.EXSTDTC, EX.EXSEQ]
    keep: first
```

Both `TRT01A` and `TRT01DOSE` then come from that same chosen record.
