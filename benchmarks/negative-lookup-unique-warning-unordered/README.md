# Reject a Repeated-Record Warning With No Record to Read

**Goal:** derive each subject's end-of-study date (`EOSDT`) from the
subject's disposition record, reporting a subject with several records
instead of stopping the run.

**Input:** demographics records with disposition records carrying category
(`DSCAT`), decoded term (`DSDECOD`), and start date (`DSSTDTC`).

**Variables:**

- `EOSDT` would be the start date of the subject's end-of-study disposition
  record, and blank when the subject has none.

**Note:** a subject with several disposition records was to be reported,
yet nothing says which of them gives the end-of-study date. Such a subject
would have no single date to read, so the specification is rejected before
any record is read and no artifact is accepted.

## How to fix

Decide which record gives a repeated subject its date, such as the latest
event, and declare that choice beside the warning:

```yaml
intermediates:
  - id: DS_EOS
    dataset: DS
    key: [STUDYID, USUBJID]
    filter: "DS.DSCAT = 'DISPOSITION EVENT'"
    order_by: [DS.DSSTDTC]
    keep: last
    verifications:
      - unique: {columns: [STUDYID, USUBJID], severity: warning}
```

When every subject must have exactly one such record, keep the check at its
default severity instead, so a repeated subject stops the run:

```yaml
    verifications:
      - unique: [STUDYID, USUBJID]
```
