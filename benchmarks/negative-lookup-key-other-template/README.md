# Reject a Device Matched on a Record Some Rows Do Not Have

**Goal:** combine blood pressure measurements taken at the clinic and at
home, with the study day in `VSDY`, where the measurement was taken in
`VSCAT`, the systolic blood pressure in `VSSTRESN`, and the method in
`VSMETHOD`.

**Input:** clinic measurements carrying the study day (`VSDY`), the systolic
blood pressure (`SYSBP`) and the device number (`DEVICEID`), home
measurements carrying the study day and the systolic blood pressure, and a
device list giving each device's method (`METHOD`).

**Variables:**

- `VSCAT` would be `CLINIC` or `HOME`.
- `VSSTRESN` would be the systolic blood pressure.
- `VSMETHOD` would be the method of the device the measurement was taken
  with.

**Note:** every record's method is found from the clinic measurement's
device number. The home measurements build some of the records, and a home
measurement has no clinic measurement of its own, so the run is rejected
before any data is read and no artifact is accepted.

## How to fix

Find the method only for the clinic measurements, and leave it blank for
the home measurements, which carry no device number:

```yaml
- id: clinic
  dataset: CLINIC
  derivations:
    VSMETHOD: DEVICE.METHOD
```

```yaml
- id: home
  dataset: HOME
  derivations:
    VSMETHOD: {literal: null}
```

Then remove the column-level `derivation` of `VSMETHOD`.
