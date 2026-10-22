# Reject an Empty Adverse Event Dataset in SAS Transport

**Goal:** submit the adverse event analysis dataset (ADAE), with the
dictionary-derived term of each event (`AEDECOD`), as the SAS transport file
`adae.xpt`.

**Input:** the collected adverse event listing of a study in which no
subject had an adverse event, so the listing holds only its header.

**Variables:**

- `AEDECOD` would be the dictionary-derived term of each adverse event.

The dataset has no rows. An empty transport file looks like a dataset that
was lost on its way into the package, so a dataset with no rows must state
that it has no data: the package then describes it as empty and carries no
transport file for it. This dataset states nothing, so the submission is
rejected before any file is written.

## How to fix

Confirm that the study collected no adverse events, then state it in the
dataset's submission metadata:

```yaml
submission:
  label: Adverse Events Analysis Dataset
  class: OCCURRENCE DATA STRUCTURE
  structure: One record per subject per adverse event
  repeating: true
  has_no_data: true
```

The data-definition document then describes ADAE as a dataset with no data,
and no `adae.xpt` is written.
