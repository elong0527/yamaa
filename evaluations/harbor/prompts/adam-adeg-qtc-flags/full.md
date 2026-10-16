Following CDISC ADaM standards, use the provided ADEG_RAW dataset to
create an ADEG dataset with one record per subject per parameter per
analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, ABLFL, ONTRTFL, BASE,
CHG, CRIT1, CRIT1FL, CRIT2, CRIT2FL, CRIT3, CRIT3FL, CRIT4, CRIT4FL,
CRIT5, CRIT5FL

Only on-treatment QTcF records are assessed: a record whose PARAMCD is
"QTCF" and whose ONTRTFL is "Y". Baseline records, records after the
on-treatment window, and other parameters, such as the uncorrected QT
interval, carry no criterion, and each flag has no value exactly where
its criterion has no value. Values are compared unrounded.

CRIT1 is "QTcF > 450 ms", CRIT2 is "QTcF > 480 ms", and CRIT3 is
"QTcF > 500 ms", each filled only on an assessed record with an AVAL.
CRIT1FL is Y when AVAL is above 450 and N otherwise, CRIT2FL is Y when
AVAL is above 480 and N otherwise, and CRIT3FL is Y when AVAL is above
500 and N otherwise, so a value exactly at a limit reads N.

CRIT4 is "QTcF increase from baseline > 30 ms" and CRIT5 is
"QTcF increase from baseline > 60 ms", each filled only on an assessed
record with a CHG, as a record without a baseline value has no CHG,
while its absolute criteria are still assessed. CRIT4FL is Y when CHG
is above 30 and N otherwise, and CRIT5FL is Y when CHG is above 60 and
N otherwise: an increase of exactly 30 or 60 reads N, and a decrease
never meets them, however large.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
