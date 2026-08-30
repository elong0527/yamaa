# ADaM ADVS: reject a record that no analysis visit identifies

This example uses a pre-derived ADVS slice to attempt one record per subject,
parameter, and analysis visit:

- `VSSEQ`, `ADT`, and `ADY` are the collected record number, the record's
  analysis date, and its study day, all carried through as given;
- `AVAL` is the value measured;
- `AVISIT` is the analysis window the record falls in, decided from the study
  day rather than from what the site called the visit. Windows run from the
  lower bound up to but not including the next, so every day belongs to exactly
  one. A record with no study day belongs to no window and is left without one.

A record is identified here by its study, subject, parameter, and analysis
window. A record left without a window carries no identity: nothing states
which measurement it is, and a later run has nowhere to put the same record.
The values are complete before that is checked, and the expected file records
the rows presented to the check, but the run still fails and no artifact is
accepted.

## How to fix

Decide which grain the dataset is on.

If it is one record per analysis visit, every record must fall in a window.
Recover the analysis date in the governed source where it is available. Where
it is not, keep the record out of the dataset rather than give it a place it
does not have. A row template selects the records that become rows, so move
one column's derivation into it and leave the rest where they are:

```yaml
- name: AVAL
  type: float

rows:
  - id: windowed
    filter: "ADVSPRE.ADY IS NOT NULL"
    derivations:
      AVAL:
        source: ADVSPRE.AVAL
```

If it is one record per collected measurement, identify rows by the collected
record instead, and a record with no window keeps an empty one:

```yaml
keys: [STUDYID, USUBJID, PARAMCD, VSSEQ]
```

Do not label the record into a placeholder window such as `NOT ASSIGNED`. That
keeps the row by asserting an analysis visit the data does not support, and a
second such record collides with the first.
