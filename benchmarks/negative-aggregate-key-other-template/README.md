# Reject a Calibration Count Matched on a Record Some Rows Do Not Have

**Goal:** combine blood pressure measurements taken at the clinic and at
home, with the study day in `VSDY`, where the measurement was taken in
`VSCAT`, the systolic blood pressure in `VSSTRESN`, and how often the device
used was calibrated in `NCAL`.

**Input:** clinic measurements carrying the study day (`VSDY`), the systolic
blood pressure (`SYSBP`) and the device number (`DEVICEID`), home
measurements carrying the study day and the systolic blood pressure, and the
calibration records of each device (`CALDY`, the day of a calibration).

**Variables:**

- `VSCAT` would be `CLINIC` or `HOME`.
- `VSSTRESN` would be the systolic blood pressure.
- `NCAL` would be the number of calibration records of the device the
  measurement was taken with.

**Note:** every record's calibration count is matched on the clinic
measurement's device number. The home measurements build some of the
records, and a home measurement has no clinic measurement of its own, so the
run is rejected before any data is read and no artifact is accepted.

## How to fix

Carry the device number on each record, blank for the home measurements,
which name no device, and count the calibrations of that value:

```yaml
- name: DEVICEID
  type: str
  label: Device Identifier
```

```yaml
NCAL:
  aggregate:
    expr: "COUNT(CALIB.CALDY)"
    key: {DEVICEID: DEVICEID}
```

The clinic template derives `DEVICEID: CLINIC.DEVICEID` and the home template
`DEVICEID: null`. A home record then matches no calibration and its count is
blank.
