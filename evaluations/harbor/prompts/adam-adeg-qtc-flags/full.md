Following CDISC ADaM standards, use the provided ADEG_RAW dataset to create an
ADEG dataset with one record per subject per parameter per analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, ABLFL, ONTRTFL, BASE, CHG,
CRIT1, CRIT1FL, CRIT2, CRIT2FL, CRIT3, CRIT3FL, CRIT4, CRIT4FL, CRIT5, CRIT5FL

Only records with PARAMCD "QTCF" and ONTRTFL "Y" are assessed. Baseline
records, records after the on-treatment window, and records of other
parameters, such as the uncorrected QT interval, carry no criterion.

- CRIT1 is "QTcF > 450 ms", CRIT2 is "QTcF > 480 ms", and CRIT3 is
  "QTcF > 500 ms" on an assessed record; each has no value when the
  record is not assessed or AVAL has no value.
- CRIT1FL is Y when AVAL is above 450, CRIT2FL is Y when AVAL is above
  480, and CRIT3FL is Y when AVAL is above 500, each N otherwise, so a
  value exactly at a limit reads N.
- CRIT4 is "QTcF increase from baseline > 30 ms" and CRIT5 is "QTcF
  increase from baseline > 60 ms" on an assessed record; each has no
  value when the record is not assessed or CHG has no value, as on a
  record with no baseline value, while its absolute criteria are still
  assessed.
- CRIT4FL is Y when CHG is above 30 and CRIT5FL is Y when CHG is above
  60, each N otherwise: an increase of exactly 30 or exactly 60 reads
  N, and a decrease never reads Y, however large.
- Each flag has no value exactly where its criterion has no value.
  AVAL and CHG are compared as they stand, without rounding.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
