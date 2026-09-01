# ADaM ADVS: carry forward a once-measured characteristic

This example uses a planned-measurement spine, long-form vital signs, and
subject treatment dates to derive one record per planned measurement:

- `ASEQ` numbers the planned measurements and `VSSEQ` identifies the collected
  record when the measurement occurred;
- `PARAMCD`, `ADT`, and `AVAL` identify the planned measurement, its analysis
  date, and its collected value. An unattended measurement has no `VSSEQ` or
  value but remains a row;
- `TRTSDT` is the subject's treatment start date;
- `HEIGHTBL` is the latest height on or before treatment. It is repeated on
  both height and weight records so later weights retain the once-measured
  subject characteristic, and is empty on every record for a subject without
  a pre-treatment height.

Weight is planned repeatedly, while height is planned only at screening. The
two parameters share the subject-level height without treating a weight as a
candidate height. A subject with no collected height has no carried value; a
later unattended weight still carries a collected baseline height.
