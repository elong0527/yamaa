# Reject a Daily Home Blood Pressure Taken From One Reading

**Goal:** combine blood pressure measured at clinic visits with blood pressure
measured at home, with the study day in `VSDY`, where the blood pressure was
measured in `VSCAT`, and the systolic blood pressure in `VSSTRESN`: one record
per clinic measurement, and one record per subject and day of home readings.

**Input:** clinic measurements, one per visit, and home readings, of which a
subject may take several on one day. Both carry the study day (`VSDY`) and the
systolic blood pressure (`SYSBP`).

**Variables:**

- `VSCAT` would be `CLINIC` for a clinic measurement and `HOME` for a day of
  home readings.
- `VSSTRESN` would be the systolic blood pressure: the measurement itself on a
  clinic record, and on a home record one value standing for all of the day's
  readings.

**Note:** each home record stands for all of the subject's readings on that
day, so a single reading's blood pressure is not a value of that record: a day
with several readings has several such values. The home record's blood
pressure is taken from a single reading, so the run is rejected before any
data is read and no artifact is accepted.

## How to fix

Decide which value stands for a day of home readings. Usually it is the mean
of the day's readings, so average them on the home record:

```yaml
VSSTRESN:
  aggregate: "MEAN(HOME.SYSBP)"
```

To keep every reading instead, build one home record per reading, without
grouping the readings by day, and add a key that tells one day's readings
apart, such as the time each reading was taken.
