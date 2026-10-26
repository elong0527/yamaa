Following CDISC ADaM standards, use the provided ADSL and QS datasets to create
an ADQS dataset with one record per subject per parameter per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, VISITNUM, VISIT, ADT, TRTSDT, AVAL, ABLFL,
SCBLFL, BASE, CHG

PARAMCD and PARAM are the collected QSTESTCD and QSTEST, unchanged. ADT is
the date in QSDTC, also on a record whose questionnaire was not done; it has
no value where QSDTC is empty, as when the visit was not attended. TRTSDT is
the subject's treatment start date. AVAL is the collected rating, QSSTRESN,
and has no value when the questionnaire was not done.

- ABLFL is "Y" on the "CYCLE 1 DAY 1" record when it has a rating and its ADT
  is on or before TRTSDT. When that record has no rating or is dated after
  TRTSDT, ABLFL is "Y" on the "SCREENING" record instead, if that has a
  rating. When neither applies, the subject has no baseline. ABLFL has no
  value on every other record.
- Only those two visits can be the baseline; a "CYCLE 2 DAY 1" rating never
  stands in.
- SCBLFL (Screening Used as Baseline Flag) is "Y" on the baseline record when
  it is the "SCREENING" record, and has no value on every other record, a
  "CYCLE 1 DAY 1" baseline included. The screening record itself carries both
  flags; no separate baseline record is added.
- BASE is the baseline record's AVAL, on every record of the subject and
  parameter, and has no value for a subject without a baseline.
- CHG is AVAL minus BASE on each record whose ADT is after TRTSDT, a
  "CYCLE 1 DAY 1" record included. CHG has no value on the baseline record and
  every other record dated on or before TRTSDT, and wherever AVAL or BASE has
  no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
