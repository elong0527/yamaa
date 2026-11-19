Following CDISC ADaM standards, use the provided ADSL, EX, LB, PC, and VS
datasets to create an ADPPK dataset with one record per administration in
EX and per sample in PC.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADTM, EVID, WT, CREAT, CRCL

- An EX administration gives a dose record: EVID is 1 and ADTM is the
  date/time of the administration (EXSTDTC).
- A PC sample gives a concentration record: EVID is 0 and ADTM is the
  date/time of the sample (PCDTC).
- ADTM is written as YYYY-MM-DDThh:mm:ss, with the seconds always shown.
- ASEQ is the record's number within the subject, from 1, in the order of
  ADTM, a sample before a dose at the same date/time.
- WT is the subject's latest weight result (VSSTRESN of the VS test
  "WEIGHT", in kg) dated on or before the date of ADTM.
- CREAT is the subject's latest creatinine result (LBSTRESN of the LB test
  "CREAT", in umol/L) dated on or before the date of ADTM.
- A result counts from its date onward, whatever the time of day of the
  record, and a result dated after the record is never used.
- A record of the test with no result, such as a weight not measured at a
  visit, is passed over, so the earlier result stays in force.
- WT or CREAT has no value when the subject has no result dated on or
  before the date of ADTM.
- CRCL is the Cockcroft-Gault creatinine clearance in mL/min, not rounded:
  (140 - AGE) x WT x 1.23 / CREAT for a male subject (SEX "M"), with 1.04
  in place of 1.23 for a female subject (SEX "F"), AGE and SEX being the
  subject's ADSL values.
- CRCL has no value when WT or CREAT has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adppk.csv.
