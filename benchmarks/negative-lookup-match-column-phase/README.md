# Reject a Period Found While the Records Are Still Being Built

**Goal:** carry each collected blood pressure measurement with the study day
in `ADY`, the record's position among the subject's records in `ASEQ`, the
systolic blood pressure in `AVAL`, and the period in `APERIODC`.

**Input:** collected measurements carrying the study day (`VSDY`) and the
systolic blood pressure (`VSSTRESN`), and the study's periods, each running
from a first (`FIRST`) to a last (`LAST`) record position.

**Variables:**

- `ASEQ` would number each subject's records from 1, by study day.
- `APERIODC` would name the period, both ends inclusive, that holds the
  record's position, and be blank when no period does.

**Note:** a record's position counts all of the subject's records, so it is
numbered only after every record is built. The period is looked up while
each record is being built, before any record has a position, and the run
is rejected before any data is read and no artifact is accepted.

## How to fix

Look the period up after the records are built, by deriving `APERIODC` for
every record once `ASEQ` is numbered:

```yaml
- name: APERIODC
  type: str
  label: Period (C)
  derivation: PERIOD.APERIODC
```

Then remove `APERIODC` from the row template's derivations.
