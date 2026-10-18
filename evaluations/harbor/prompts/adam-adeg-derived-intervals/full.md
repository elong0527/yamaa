Following CDISC ADaM standards, use the provided ADEG dataset to create an ADEG
dataset with one record per subject, parameter, and analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, AVALU

The output holds every collected record and adds one new record per
subject and analysis visit for each derived parameter. A new record with
PARAMCD QTCBR has PARAM "QTcB - Bazett's Correction Formula Rederived
(ms)". A new record with PARAMCD QTCFR has PARAM "QTcF - Fridericia's
Correction Formula Rederived (ms)". A new record with PARAMCD RRR has
PARAM "RR Duration Rederived (ms)". AVALU is "ms" on the new records.

QT and RR results are in milliseconds, with the unit shown as "ms" or
"msec", and the heart rate is in beats per minute. For QTCBR, AVAL is
QT / SQRT(RR / 1000), and for QTCFR, AVAL is QT / POWER(RR / 1000,
1 / 3), where QT and RR are the results for the same subject and
analysis visit. A QTCBR or QTCFR record is added only when both the QT
and the RR results are present for that subject and analysis visit.
For RRR, AVAL is 60000 divided by the heart rate for the same subject
and analysis visit, so a heart rate of 60 gives 1000. An RRR record is
added only when the heart rate is present and not zero. AVAL on the new
records is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
