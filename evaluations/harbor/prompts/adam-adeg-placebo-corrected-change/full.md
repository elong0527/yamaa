Following CDISC ADaM standards, use the provided ADEG_RAW dataset to create an
ADEG dataset with one record per subject, parameter, analysis period, nominal
day and nominal time point.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, COHORT, TRTA, PARAMCD, PARAM, APERIOD, APERDAY, ATPT, ATPTN,
AVAL, AVALU, ABLFL, BASE, CHG, BASETYPE, ACOMPFL, COMP, COMPBASE, COMPCHG,
CCOMPCHG, COMPTYPE

- A record with TRTA of "Placebo" is a placebo record; every other record
  is a drug record.
- A baseline record is a record with ABLFL of "Y", from the baseline day,
  the day before dosing; every other record is after baseline.
- ACOMPFL is "Y" on every placebo record after baseline, and has no value
  on drug records and on baseline records.
- COMPTYPE is "Placebo group at " followed by the record's ATPT, on every
  record after baseline, drug and placebo alike, also when no placebo
  record exists at that parameter and time point; COMPTYPE has no value
  on baseline records.
- The placebo group a record is compared with holds every placebo record
  after baseline with the same PARAMCD and the same ATPTN, from all
  cohorts together.
- COMP is the mean AVAL of that group on every record after baseline;
  COMP has no value when the group holds no record, and on baseline
  records.
- COMPBASE is the mean BASE of the same group, over the records that have
  a BASE; COMPBASE has no value when no record of the group has one, and
  on baseline records.
- COMPCHG is the mean CHG of the same group, over the records that have a
  CHG, so it is not always COMP minus COMPBASE; COMPCHG has no value
  when no record of the group has one, and on baseline records.
- CCOMPCHG is CHG minus COMPCHG on drug records after baseline, and has
  no value on placebo records and on baseline records, or when CHG or
  COMPCHG has no value.
- A placebo record carries its group's means, not its own values, and
  none of the comparator values is rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
