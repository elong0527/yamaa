# Reject Device Calibrations Matched on a Number the Dataset Does Not Carry

**Goal:** count the calibrations (`NCAL`) of the device each blood pressure
measurement (`VSSTRESN`) was taken with.

**Input:** collected measurements, each with the number of the device that
took it, and a log of device calibrations by device number.

**Variables:**

- `NCAL` would be the number of times the measurement's device was
  calibrated.

**Note:** the calibrations are matched on a value of the row being built.
The device number is a field of the collected record that the dataset does
not carry, so the run is rejected before any data is read; the field is
never read in its place, and no artifact is accepted.

## How to fix

Read the device number from the record each row is built from, naming the
dataset:

```yaml
aggregate:
  expr: "COUNT(CALIB.CALDT)"
  key: {DEVICEID: VS_RAW.DEVNUM}
```

Or bring the device number onto the row as a column and name the column.
