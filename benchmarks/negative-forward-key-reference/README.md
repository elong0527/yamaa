# Reject a Baseline Flag Listed Before Its Visit Number

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

**Note:** visits are numbered only after every analysis record has been
built, so the visit number is available only to values listed after it,
exactly like any other value. The baseline flag is listed before the visit
number it reads, so the run is rejected before any data is read and no
artifact is accepted.

## How to fix

List `AVISITN` before `ABLFL`, so the flag reads a visit number that has
already been counted:

```yaml
- name: AVISITN
  type: int
  derivation:
    rank:
      method: dense
      window:
        group_by: [STUDYID, USUBJID]
        order_by: [ADT]
- name: ABLFL
  type: str
  derivation:
    flag: "AVISITN = 1"
```
