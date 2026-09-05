# ADaM ADLB: build a BDS dataset with baseline and change

This example uses collected lab data with ADSL and a `yamaa` specification to
derive one row per subject, parameter, and visit:

- `PARAMCD` and `PARAM` name the analysis parameter. Each collected test
  becomes one, and a parameter may also be derived from another: alanine
  aminotransferase is reported both as collected and converted to SI units;
- `ADT` is the collection date, and `AVAL` and `AVALU` the analysis value and
  its unit;
- `TRTSDT` and `TRT01A` are the subject's treatment start date and treatment,
  carried across from ADSL without changing how many records there are;
- `ABLFL` marks the baseline record for each subject and parameter: the latest
  record on or before treatment start. `BASE` repeats that record's value on
  every record for the parameter, so each visit can be compared with it;
- `CHG` is the change from baseline and `PCHG` the percentage change. A subject
  whose baseline is zero has a change but no percentage change, since the
  percentage is not defined;
- `ASEQ` numbers a subject's records once they all exist.

A collected result with no numeric value produces no record, so it contributes
neither its own parameter nor any parameter derived from it.

A subject listed in ADSL with no collected result produces no record at
all. The treatment start date and treatment carried across from ADSL
enrich records that already exist and never bring one into being, so the
row count follows the collected lab data alone.
