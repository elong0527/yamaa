# Reject a Planned Dose Found While the Records Are Still Being Built

**Goal:** carry each administration with its study day in `ASTDY`, its
number among the subject's administrations in `ADOSENUM`, the dose given in
`AVAL`, and the dose the schedule plans for that administration in
`PLANDOSE`.

**Input:** exposure records, one per administration, carrying the study day
(`EXSTDY`) and the dose given (`EXDOSE`), and a dosing schedule giving the
planned dose (`PLANDOSE`) of each administration number (`DOSENUM`).

**Variables:**

- `ADOSENUM` would number each subject's administrations from 1, by study
  day.
- `PLANDOSE` would be the planned dose of the administration's number, and
  blank when the schedule plans none.

**Note:** an administration's number counts all of the subject's
administrations, so it is numbered only after every record is built. The
planned dose is looked up while each record is being built, before any
administration has a number, so the run is rejected before any data is read
and no artifact is accepted.

## How to fix

Look the planned dose up after the records are built, by deriving `PLANDOSE`
for every record once `ADOSENUM` is numbered:

```yaml
- name: PLANDOSE
  type: float
  label: Planned Dose
  derivation: PLANNED.PLANDOSE
```

Then remove `PLANDOSE` from the row template's derivations.
