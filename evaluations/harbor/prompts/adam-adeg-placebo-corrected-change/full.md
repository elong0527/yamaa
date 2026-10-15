Following CDISC ADaM standards, use the provided ADEG_RAW dataset to
create an ADEG dataset with one record per subject per parameter per
analysis period per nominal period day per nominal time point.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, COHORT, TRTA, PARAMCD, PARAM, APERIOD, APERDAY, ATPT,
ATPTN, AVAL, AVALU, ABLFL, BASE, CHG, BASETYPE, ACOMPFL, COMP, COMPBASE,
COMPCHG, CCOMPCHG, COMPTYPE

The study is placebo-controlled and parallel-group, given in dose cohorts
with placebo subjects in every cohort; each subject is dosed once. Each
input record is the average of the replicate ECGs at one nominal time point
and gives one output record, and its time-matched baseline (ABLFL, BASE,
CHG) is already in place; the records with ABLFL "Y" are the baseline
records. Add the time-matched placebo comparator:
- ACOMPFL is "Y" on every placebo record (TRTA "Placebo") that is not a
  baseline record, and has no value on every other record.
- The placebo group of a record is every record with ACOMPFL "Y" of the
  same parameter at the same nominal time point (ATPTN), pooled over all
  cohorts, not only the record's own cohort.
- COMP is the mean AVAL of the placebo group, COMPBASE its mean BASE, and
  COMPCHG its mean CHG. Each mean is over the group's records that have a
  value in that column, so COMPCHG is not always COMP minus COMPBASE, and
  has no value when no group record has one.
- Every record that is not a baseline record, drug and placebo alike,
  carries its placebo group's means: a placebo record carries the group
  means, not its own AVAL, BASE and CHG.
- CCOMPCHG is CHG minus COMPCHG on the records that are neither baseline
  nor placebo records, and has no value when CHG or COMPCHG has no value.
  Placebo records have no CCOMPCHG.
- COMPTYPE is "Placebo group at ", then ATPT, for example
  "Placebo group at 2 h", on every record that is not a baseline record,
  also when the placebo group has no record.
- Baseline records have no ACOMPFL, COMP, COMPBASE, COMPCHG, CCOMPCHG or
  COMPTYPE.
- COMP, COMPBASE, COMPCHG and CCOMPCHG are not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
