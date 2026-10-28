Following CDISC SDTM standards, use the provided ODM dataset to create an EG
dataset with one record per test of each replicate ECG taken at a planned
time point.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, EGSEQ, EGTESTCD, EGTEST, EGORRES, EGORRESU, EGSTRESC,
EGSTRESN, EGSTRESU, EGREPNUM, VISIT, EGDTC, EGTPT, EGTPTNUM

DOMAIN is "EG". VISIT is "DAY 1". EGTPT is "PRE-DOSE" with EGTPTNUM 1, or
"2 H POST-DOSE" with EGTPTNUM 2. EGTESTCD is "QTAG" with EGTEST "QT
Interval, Aggregate", or "INTP" with EGTEST "Interpretation". EGORRESU and
EGSTRESU are "msec" or have no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/eg.csv.
