Following CDISC ADaM standards, use the provided ADEG_RAW dataset to
create an ADEG dataset with one record per subject per parameter per
analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, ABLFL, ONTRTFL, BASE,
CHG, CRIT1, CRIT1FL, CRIT2, CRIT2FL, CRIT3, CRIT3FL, CRIT4, CRIT4FL,
CRIT5, CRIT5FL

CRIT1 is "QTcF > 450 ms", CRIT2 is "QTcF > 480 ms", and CRIT3 is
"QTcF > 500 ms". CRIT4 is "QTcF increase from baseline > 30 ms" and
CRIT5 is "QTcF increase from baseline > 60 ms". CRIT1FL, CRIT2FL,
CRIT3FL, CRIT4FL, and CRIT5FL are Y or N or have no value. Values are
compared unrounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
