# Reject Repeated End-of-Study Records

**Goal:** derive each subject's end-of-study date (`EOSDT`) from the
subject's one end-of-study disposition record.

**Input:** demographics records with disposition records carrying
category (`DSCAT`), decoded term (`DSDECOD`), and start date
(`DSSTDTC`).

**Variables:**

- `EOSDT` would be the start date of the subject's end-of-study
  record, the disposition event that is not a screen failure, and
  blank when the subject has none. The study expects at most one
  such record per subject; a subject with two has no single
  end-of-study date, so the run is rejected before any output row is
  built and no artifact is accepted.

## How to fix

Give each subject exactly one eligible end-of-study record in the
source data. When the source cannot promise that, drop the uniqueness
assertion and choose the record explicitly instead, here the one with the
latest start date:

```yaml
intermediates:
  - id: DS_EOS
    dataset: DS
    key: [STUDYID, USUBJID]
    filter: "DS.DSCAT = 'DISPOSITION EVENT' AND DS.DSDECOD <> 'SCREEN FAILURE'"
    order_by: [DS.DSSTDTC]
    keep: last
```

The `no_match` line leaves `EOSDT` blank for a subject with no
eligible record, one whose only disposition records are screen
failures, instead of failing the lookup.

Keep the uniqueness assertion only for eligible sets that truly
carry each key once.
