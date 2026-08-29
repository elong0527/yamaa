# SDTM EX: represent a combination regimen

This example uses collected exposure records from a combination oncology
regimen and a `yamaa` specification to derive one record per administered
component:

- `EXTRT`, `EXDOSE`, and `EXDOSU` identify the component, its collected dose,
  and the unit appropriate to that component. Milligrams, milligrams per square
  metre, and target area under the curve remain distinct units;
- `EXSTDTC` and `EXENDTC` give the administration interval;
- `EXADJ` records a dose adjustment when one was reported and is empty
  otherwise;
- `EXSEQ` numbers administrations by date and then treatment within subject.

A saline placebo dose of zero is an administered component and remains zero;
it is not treated as an uncollected dose.
