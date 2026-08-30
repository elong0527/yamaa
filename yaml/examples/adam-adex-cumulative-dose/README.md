# ADaM ADEX: summarize cumulative exposure

This example uses a subject-treatment inventory with its component exposure
records to derive one record per subject and treatment:

- `EXTRT` identifies the regimen component and `EXDOSU` its dose unit;
- `DOSECUM` is the sum of administered doses and `NCYCLES` is the number of
  administration records, including an administered zero dose;
- `RDI` is cumulative dose as a percentage of planned dose across the planned
  cycles. It is empty when the planned total dose is zero.

`DOSECUM` and `NCYCLES` use qualified `EX` aggregate expressions without an
explicit `group_by`. They therefore reduce the exposure records by the
applicable output keys (`STUDYID`, `USUBJID`, and `EXTRT`) before joining the
results to each subject-treatment row; omission does not reduce all `EX`
records as one group.
