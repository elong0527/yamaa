Following CDISC ADaM standards, use the provided EG dataset to create an ADEG
dataset with one record per subject, planned time point and parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ATPT, ATPTN, DTYPE, AVAL

PARAMCD is QTAG, RRAG, or QTCF. PARAM is "QT Interval, Aggregate (msec)"
when PARAMCD is QTAG, "RR Interval, Aggregate (msec)" when PARAMCD is
RRAG, and "QTcF Interval, Aggregate (msec)" when PARAMCD is QTCF. DTYPE is
AVERAGE on every record. AVAL is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
