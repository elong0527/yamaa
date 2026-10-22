# Reject Unchosen Record

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

## How to fix

State the full order and which end of that order to keep. To choose the
earliest administration, read the sequence as an integer so ties on the
date are broken numerically:

```yaml
input:
  EX: {path: input/ex.csv, types: {EXSEQ: int}}

intermediates:
  - id: DOSING
    dataset: EX
    key: [STUDYID, USUBJID]
    order_by: [EX.EXSTDTC, EX.EXSEQ]
    keep: first
```

Both `TRT01A` and `TRT01DOSE` then come from that same chosen record.
