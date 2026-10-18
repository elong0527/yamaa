Following CDISC ADaM standards, use the provided EG dataset to create an ADEG
dataset with one record per subject, planned time point and parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ATPT, ATPTN, DTYPE, AVAL

PARAMCD is QTAG, RRAG, or QTCF. PARAM is "QT Interval, Aggregate (msec)"
when PARAMCD is QTAG, "RR Interval, Aggregate (msec)" when PARAMCD is
RRAG, and "QTcF Interval, Aggregate (msec)" when PARAMCD is QTCF. ATPT
and ATPTN are the planned time point name and number (EGTPT and
EGTPTNUM) of the ECGs averaged. DTYPE is AVERAGE on every record.

AVAL is the mean, over the time point's ECGs that have a value for the
parameter, of that value, and is not rounded. For QTAG the value of one
ECG is its QT result, and for RRAG it is its RR result. An ECG whose
test was not done has no result and does not enter the mean. For QTCF
the value of one ECG is its own QT divided by the cube root of its own
RR in seconds, so AVAL is the mean of the per-ECG corrected values, not
the correction of the mean QT and RR. The QT and RR results of one ECG
are the pair that shares its replicate number within the subject and
time point, even when two ECGs carry the same collection date and time.
An ECG without both results, or with an RR of zero, has no corrected
value. A single ECG at a time point gives a record of its own values.
A parameter with no value at a time point has no record there.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
