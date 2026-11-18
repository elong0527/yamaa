# Compare laboratory results with baseline with ADaMIG 1.1

Goal: Carry the flagged baseline value and compute the change for each
laboratory result.

Input: SDTM laboratory results and subject-level treatment dates and names.

Variables:

- `PARAMCD` and `PARAM` identify alanine aminotransferase; `PARAM` names
  its unit, U/L.
- `ADT` is the collection date and `ADY` is its day relative to first
  treatment, with no day zero.
- `TRT01A` carries the subject's treatment.
- `AVAL` retains the standardized result.
- `ABLFL` retains the baseline flag.
- `BASE` repeats the flagged value for the subject and parameter.
- `CHG` is the result minus baseline on a record after the first treatment
  date; it is absent on the baseline record, before treatment, and when
  baseline is absent.

Note: `project.yaml` declares ADaMIG 1.1. The data-definition document
records that guide and points to the SAS Transport version 5 dataset.
