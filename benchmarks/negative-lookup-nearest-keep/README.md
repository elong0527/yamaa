# Reject Two Ways of Choosing the Sample for an ECG

**Goal:** give each electrocardiogram (ECG) the sequence number (`PCSEQ`)
and concentration (`PCCONC`) of the subject's plasma sample nearest in time
to it.

**Input:** ECG records, each with its sequence number and date and time,
and pharmacokinetic (PK) concentration records, each with its sequence
number, date/time and result.

**Variables:**

- `PCSEQ` would be the sequence number of the subject's sample nearest in
  time to the ECG.
- `PCCONC` would be the concentration of that sample.

**Note:** the sample is to be chosen as the one nearest the ECG, yet the
earlier choice of the subject's latest sample is still stated beside it.
The two choices can disagree, and nothing says which one wins, so the
specification is rejected before any record is read and no artifact is
accepted.

## How to fix

Decide which sample an ECG takes. When it is the sample nearest in time,
remove the order and the choice of the last record:

```yaml
intermediates:
  - id: NEAREST
    dataset: PC
    key: [STUDYID, USUBJID]
    nearest: {source: PCDTM, to: EG.EGDTC, unit: minute}
    columns: [PCSEQ, PCSTRESN]
```

When it is the subject's latest sample, keep `order_by` and `keep` and
remove `nearest`.
