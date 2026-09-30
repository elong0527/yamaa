Following CDISC ADaM standards, use the provided ADEG_RAW dataset to
create an ADEG dataset with one record per subject per parameter per
analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, AVALU

Keep every collected record unchanged: QT, RR, and HR with their labels,
results, and units. Add one new record per subject and visit for each
measure the visit supports:
- "QTCBR" with PARAM "QTcB - Bazett's Correction Formula Rederived (ms)":
  the QT result divided by the square root of the RR result expressed in
  seconds;
- "QTCFR" with PARAM
  "QTcF - Fridericia's Correction Formula Rederived (ms)": the QT result
  divided by the cube root of the RR result expressed in seconds;
- "RRR" with PARAM "RR Duration Rederived (ms)": the RR duration in
  milliseconds matching the collected heart rate.
A QTc record is added only when both the QT and the RR results are
present at that visit. An RRR record is added only when the heart rate
is present and not zero. New records use AVALU "ms", and their AVAL is
not rounded. The input holds no QTCBR, QTCFR, or RRR records.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
