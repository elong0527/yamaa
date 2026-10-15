Following CDISC ADaM standards, use the provided EG dataset to create an
ADEG dataset with one record per subject per parameter per analysis time
point.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ATPT, ATPTN, DTYPE, AVAL

DTYPE is "AVERAGE" on every record. The parameters are:
- "QTAG" with PARAM "QT Interval, Aggregate (msec)";
- "RRAG" with PARAM "RR Interval, Aggregate (msec)";
- "QTCF" with PARAM "QTcF Interval, Aggregate (msec)", by Fridericia's
  correction.
AVAL is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
