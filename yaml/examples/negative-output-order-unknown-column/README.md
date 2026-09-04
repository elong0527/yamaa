# SDTM VS: reject a submission order over a value the dataset does not carry

This example uses collected vital signs to attempt one record per measurement:

- `VSTESTCD` names the measurement, and `VSSTRESN` is its result in standard
  units.

The records are to be presented by subject and then by the visit they were
collected at, but the visit number is only a collected field: no variable of
this dataset carries it, so no completed record has a value to be ordered by.
Ordering by something the records do not contain has no meaning the run could
give it, so the run must fail before any data is read and no artifact is
accepted.

## How to fix

Decide whether the visit number belongs in the dataset. When the order must
rest on it, declare it as a variable and order by that variable; place it in
`output.columns` only when the artifact should carry it:

```yaml
output:
  columns: [STUDYID, USUBJID, VSSEQ, VSTESTCD, VSSTRESN]
  order_by: [USUBJID, VISITNUM]

columns:
  - name: VISITNUM
    type: int
    label: Visit Number
    derivation:
      source: VS_RAW.VISITNUM
```

When the visit number is not wanted at all, order by a variable the dataset
already declares, such as `VSSEQ`.
