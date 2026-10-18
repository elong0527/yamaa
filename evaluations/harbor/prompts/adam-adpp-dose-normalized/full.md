Following CDISC ADaM standards, use the provided EX and PP datasets to create
an ADPP dataset with one record per subject, parameter, and visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL, AVALU

- Every collected record is in the output, and a dose-normalized record is
  added for each CMAX, AUCLST, and AUCIFO record.
- PARAMCD on an added record is CMAXD for a CMAX record, AUCLSTD for an
  AUCLST record, and AUCIFOD for an AUCIFO record.
- PARAM on an added record is "Max Conc Norm by Dose" for CMAXD, "AUC to
  Last Nonzero Conc Norm by Dose" for AUCLSTD, and "AUC Infinity Obs Norm
  by Dose" for AUCIFOD.
- AVAL is the result in standard units and has no value when the parameter
  was not calculated.
- AVAL on an added record is that result divided by the dose, in
  milligrams, of the one administration of the same subject that starts
  at the profile's reference time.
- A dose in micrograms counts as a thousandth of a milligram.
- A twice-daily day divides by one administration's dose, not the day's
  total.
- AVALU is the standard unit on a collected record and that unit per mg on
  an added record, "ng/mL/mg" for a CMAXD record and "h*ng/mL/mg" for an
  AUCLSTD or AUCIFOD record.
- AVAL on an added record is not rounded.
- No dose-normalized record is added for any other parameter, for a
  parameter that has no value, or for a profile whose reference time has
  no administration or whose administration's dose is zero.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
