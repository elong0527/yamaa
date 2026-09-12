# ADaM ADVS: carry forward a once-measured characteristic

This example uses a planned-measurement spine, long-form vital signs, and
subject treatment dates to derive one record per planned measurement:

- `ASEQ` numbers the planned measurements and `VSSEQ` identifies the collected
  record when the measurement occurred;
- `PARAMCD` and `ADT` identify the planned measurement and its analysis date;
- `AVAL` is the collected value when present, otherwise the most recent
  earlier collected value for that subject and parameter. It remains empty
  before the first collected value;
- `TRTSDT` is the subject's treatment start date;
- `HEIGHTBL` is the latest height on or before treatment. It is repeated on
  both height and weight records so later weights retain the once-measured
  subject characteristic, and is empty on every record for a subject without
  a pre-treatment height.

Weight is planned repeatedly, while height is planned only at screening. A
weight can cross any number of unattended planned measurements, but never
crosses subjects or parameters. `HEIGHTBL` instead broadcasts one selected
height across both parameters. A subject with no collected height has no
baseline height even when later weights are available.
