# Reject a Saved Subject-Level Dataset Older Than Its Description

**Goal:** carry each subject's first dose date (`TRTSDT`) onto every
adverse event record.

**Input:** collected adverse events, one record per event, and a copy of
the subject-level analysis dataset as it was last saved. `spec_adsl.yaml`
produces that dataset and describes its columns.

**Variables:**

- `AESEQ` would be the collected sequence number of the event.
- `AEDECOD` would be the dictionary-derived term as collected.
- `TRTSDT` would be the subject's first dose date from the subject-level
  dataset.

**Note:** the copy was saved before its producer added the first dose date,
so the file has no `TRTSDT` column, while its description promises one. The
file no longer matches what describes it, so the run is rejected and no
artifact is accepted.

## How to fix

Rebuild and save the subject-level dataset from `spec_adsl.yaml`, then read
the file that build saves instead of the old copy:

```yaml
ADSL:
  path: adsl.csv
  schema: spec_adsl.yaml
  key: [STUDYID, USUBJID]
```

Do not edit the old copy's header by hand: only a new build makes a saved
file match its producer.
