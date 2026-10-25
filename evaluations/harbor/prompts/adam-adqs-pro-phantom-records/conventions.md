Following CDISC ADaM standards, use the provided ADSL, QS, and SCHEDULE
datasets to create an ADQS dataset with one record per subject per parameter
per planned visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARCAT2, PARAMCD, PARAM, AVISIT, AVISITN, AVAL, DTYPE,
QSSTAT, QSREASND, AREASND, PROEX1FL, PROEX2FL, PROSCMFL

The parameters are items FAT01 and FAT02 and total score FATTOT, and PARCAT2
is "ITEM" or "TOTAL SCORE". DTYPE is "PHANTOM" or has no value. AREASND is
"NOT TREATED", the reason treatment ended (DCTREAS), a QSREASND,
"NOT CALCULABLE", or has no value. PROEX1FL, PROEX2FL and PROSCMFL are "Y"
or have no value. VISITDY and an event's day are both counted from RANDDT as
day 1.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
