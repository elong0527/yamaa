# Reject a Device Questionnaire Placed by a Clinic Date

**Goal:** give each questionnaire its treatment period (`APERIOD`), next to
where it was completed (`SOURCE`), its sequence number (`QSSEQ`) and its date
(`QSDT`).

**Input:** questionnaires completed at clinic visits, questionnaires
completed on an electronic patient-reported outcome (ePRO) device, which
store their fields in a different order, and a table giving each subject's
treatment periods, each from a first to a last day.

**Variables:**

- `SOURCE` would be `CLINIC` for a questionnaire completed at a visit and
  `EPRO` for one completed on the device.
- `APERIOD` would be the number of the subject's period whose first and last
  days, both inclusive, hold the questionnaire's own date, and blank when no
  period does.

**Note:** a device questionnaire is placed by a value of the device row
itself: a field of the device record the row was built from, or a column
built for that row. The date is taken from a clinic record instead, which a
device row does not have, so the run is rejected before any data is read; no
artifact is accepted.

## How to fix

Decide which date places a device questionnaire; usually it is the date the
device recorded. Read that date from the device record the row is built
from, as the clinic template reads its own record:

```yaml
between: {value: EPRO.QSDT, lower: APERSDT, upper: APEREDT}
```

Or name the column `QSDT`, which each template already fills from its own
record: a bare name reads that column of the row.
