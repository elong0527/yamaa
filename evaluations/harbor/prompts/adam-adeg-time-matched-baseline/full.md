Following CDISC ADaM standards, use the provided ADEG_RAW dataset to
create an ADEG dataset with one record per subject per parameter per
analysis period per nominal period day per nominal time point.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, APERIOD, APERDAY, ATPT, ATPTN, AVAL,
AVALU, ABLFL, BASE, CHG, BASETYPE

Each input record is the average of the replicate ECGs at one nominal time
point and gives one output record. The baseline is time-matched, and every
period has its own baseline day: day -1 of that period (APERDAY -1).
- ABLFL is "Y" on every record of a baseline day and has no value on the
  other records.
- BASE on a baseline-day record is its own AVAL. On every other record,
  BASE is the AVAL of the baseline-day record of the same subject,
  parameter and period at the same nominal time point (ATPTN). BASE has no
  value when that baseline day has no record at that time point or the
  period has no baseline day: no other time point, day or period, and no
  pre-dose value of the same day, takes its place.
- CHG is AVAL minus BASE. It has no value on a baseline-day record and
  when BASE has no value.
- BASETYPE is "Period ", then APERIOD as a whole number, then
  " Baseline Day ", then ATPT, for example "Period 2 Baseline Day 4 h". It
  is filled on every record, also where BASE has no value.
- BASE and CHG are not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
