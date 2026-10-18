# Reject a Baseline Flag Set Before Visits Are Numbered

**Goal:** turn each collected visit's blood pressure into one systolic and
one diastolic analysis record, number each subject's visits in `AVISITN`,
and flag the records of the subject's first visit in `ABLFL`.

**Input:** collected vital signs with one record per subject and visit,
carrying the visit date (`VSDTC`) and the systolic (`SYSBP`) and diastolic
(`DIABP`) blood pressure.

**Variables:**

- `PARAMCD` would be `SYSBP` on the systolic record and `DIABP` on the
  diastolic record of each visit.
- `AVISITN` would number each subject's visits from 1 in date order; the
  two records of one visit share its number.
- `ADT` would be the visit date.
- `AVAL` would be the systolic or diastolic blood pressure the record
  names; missing when the visit did not collect it.
- `ABLFL` would be `Y` on the records of the subject's first visit, which
  this study takes as baseline, and blank on every other record.

**Note:** visits can be numbered only once every analysis record has been
built, because the numbering spans the records of both measurements. The
baseline flag is set on each record while that record is being built, so
the visit number it reads does not exist yet. The run is rejected before
any data is read and no artifact is accepted.

## How to fix

Flag the baseline after the records are built: give `ABLFL` one column-level
derivation, listed after `AVISITN`, and remove it from both row templates:

```yaml
- name: ABLFL
  type: str
  label: Baseline Record Flag
  derivation:
    flag:
      condition: "AVISITN = 1"
```
