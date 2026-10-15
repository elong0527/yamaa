Following CDISC ADaM standards, use the provided EG dataset to create an
ADEG dataset with one record per subject per parameter per analysis time
point.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ATPT, ATPTN, DTYPE, AVAL

Every record is an average over the replicate ECGs of one subject at one
planned time point; the individual ECGs get no records of their own.
ATPT and ATPTN are the planned time point name and number, and DTYPE is
"AVERAGE" on every record. One ECG is one replicate number (EGREPNUM) of
a subject at a planned time point: its QT and RR results share that
number, even when two ECGs carry the same collection date and time. The
parameters are:
- "QTAG" with PARAM "QT Interval, Aggregate (msec)": the mean QT result;
- "RRAG" with PARAM "RR Interval, Aggregate (msec)": the mean RR result;
- "QTCF" with PARAM "QTcF Interval, Aggregate (msec)": the mean of the
  QTcF values of the ECGs, where the QTcF of one ECG is Fridericia's
  correction of its own results: its QT result divided by the cube root
  of its RR result expressed in seconds (milliseconds divided by 1000).
  It is not the correction of the mean QT by the mean RR.
Each mean is taken over the ECGs of the time point that have a value for
that parameter: a test that was not done has no value, and an ECG without
both a QT and an RR result has no QTcF value while its other result still
counts in its own mean. An ECG with zero RR also has no QTcF value. A time
point with a single ECG still has its records. A parameter with no value
at a time point has no record there.
AVAL is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
