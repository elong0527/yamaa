# Refuse an adverse event list in no stated order

**Goal:** build one subject-level record per subject carrying `AETERMS`, the
subject's adverse event terms written as one text.

**Input:** demographics with one record per subject, and collected adverse
events with their reported terms.

**Variables:**

- `AETERMS` would list the subject's reported adverse event terms, separated
  by a comma and a space.

**Note:** nothing states which term the list starts with. The order in which
records happen to be stored is not a rule, and the same events stored in
another order would give another text, so the run is rejected before any data
is read.

## How to fix

Decide the order the terms are listed in, and state it. Alphabetical order
suits a list of terms:

```yaml
- name: AETERMS
  type: str
  derivation:
    aggregate:
      key: [STUDYID, USUBJID]
      order_by: [AE.AETERM]
      separator: ", "
      expr: "STR_JOIN(AE.AETERM)"
```

To list the events in the order they were recorded, order them by their
sequence number, `AE.AESEQ`, instead. Records equal on every stated term keep
their stored order.
