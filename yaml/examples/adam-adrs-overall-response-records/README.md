# ADaM ADRS: prepare the overall response records an endpoint reads

This example uses collected tumour assessments and the subject's treatment
start to derive one record per overall response recorded by the investigator:

- `RSDTC` is the assessment date as collected, which may name only a year and
  a month;
- `ADT` is that date completed, taking the first day of any period the
  collection did not narrow, and `ADY` is its study day with the first day of
  treatment as day one;
- `AVALC` is the assessment and `AVAL` its rank, `1` for a complete response
  through `6` for an assessment that was not evaluable. Responses have no order
  of their own, so the rank is what makes one assessment worse than another;
- `ANL01FL` marks one record at each assessment date: the worst response
  recorded that day, and the earliest of those when a day carries the same
  response twice.

Assessments of an individual lesion, and assessments made by anyone other than
the investigator, are not overall responses and leave no record here.

Completing a date that was collected without a day leaves no trace: `ADT` on
such a record is a complete date like any other, and every later comparison
treats it as one.
