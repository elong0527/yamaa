# Replicate ECG Averages with QTcF Corrected per ECG

**Goal:** one average record (`DTYPE` `AVERAGE`) per subject (`USUBJID`),
planned time point (`ATPTN`) and parameter (`PARAMCD`) over the replicate
electrocardiograms (ECGs) taken at that time point: the QT interval, the
RR interval, and the QT interval corrected for heart rate by Fridericia's
formula (QTcF).

**Input:** SDTM ECG test results, one record per ECG test per replicate
per planned time point. Each ECG carries an aggregate QT result
(`QTAG`) and an aggregate RR result (`RRAG`) in milliseconds; a test
that was not done has `EGSTAT` `NOT DONE` and no result.

**Variables:**

- `PARAMCD`: `QTAG`, `RRAG`, or `QTCF`.
- `PARAM`: "QT Interval, Aggregate (msec)", "RR Interval, Aggregate
  (msec)", or "QTcF Interval, Aggregate (msec)".
- `ATPT` and `ATPTN`: the planned time point name and number of the
  ECGs averaged (`EGTPT`, `EGTPTNUM`).
- `DTYPE`: `AVERAGE` on every record.
- `AVAL`: the mean, over the time point's ECGs that have a value for the
  parameter, of that value, not rounded. For `QTCF` the value of one ECG
  is its own QT divided by the cube root of its own RR in seconds, so
  the average is the mean of the per-ECG corrected values, not the
  correction of the mean QT and RR; an ECG without both results has no
  corrected value. A single ECG gives a record of its own values.

**Note:** the QT and RR results of one ECG share its replicate number
(`EGREPNUM`) within the time point, even when two ECGs carry the same
collection date and time. A parameter with no value at a time point has
no record there.
