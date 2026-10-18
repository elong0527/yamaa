Following CDISC ADaM standards, use the provided ADEG_RAW dataset to create an
ADEG dataset with one record per subject, parameter, analysis period,
nominal day within the period, and nominal time point.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, APERIOD, APERDAY, ATPT, ATPTN, AVAL, AVALU,
ABLFL, BASE, CHG, BASETYPE

- ABLFL is "Y" on every record of day -1 of a period, that period's baseline
  day; it has no value on every other day.
- BASE is the AVAL recorded at the same nominal time point on the same
  period's baseline day for the same subject and parameter; on the baseline
  day, BASE is the record's own AVAL.
- BASE has no value when that baseline day did not record the time point or
  the period has no baseline day; nothing stands in for it, neither another
  time point, day or period nor the dosing day's pre-dose value.
- CHG is AVAL minus BASE and is not rounded; CHG has no value on the baseline
  day or wherever BASE has no value.
- BASETYPE is "Period " followed by the period number, " Baseline Day " and
  ATPT on every record, including those with no baseline.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
