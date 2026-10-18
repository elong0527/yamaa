Following CDISC ADaM standards, use the provided PC and PP datasets to create
an ADPP dataset with one record per subject, period, and pharmacokinetic
parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISITNUM, VISIT, PARAMCD, PARAM, AVAL, PRECONC, PREPCMAX,
ANL01FL

- VISITNUM and VISIT name the dosing period the parameter belongs to, as
  PP gives them, and PARAMCD and PARAM are the parameter's test code and
  test name.
- AVAL is the parameter's numeric result in standard units.
- PRECONC is the concentration of the period's pre-dose sample: the PC
  record for the same subject planned at "PREDOSE" whose reference dose
  time is the parameter's.
- A pre-dose result below the lower limit of quantitation, recorded as a
  character result beginning with "<", counts as 0.
- PRECONC has no value when the period has no pre-dose sample or the
  sample was not done, and no other sample stands in for it.
- PREPCMAX is PRECONC as a percentage of the same subject's Cmax in the
  same period, the AVAL of the period's "CMAX" parameter, and is not
  rounded.
- PREPCMAX has no value when PRECONC has no value, and has no value when
  the period's Cmax is zero.
- ANL01FL has no value on every parameter of a period whose PREPCMAX is
  greater than 5, and is "Y" on every other record, including a period
  whose PREPCMAX has no value.
- Exactly 5 percent stays in.
- Every period is checked, the first one included, and the period is the
  unit left out: the subject's other period keeps "Y" and every record
  stays in the dataset.
- PRECONC, PREPCMAX and ANL01FL repeat on every parameter of the period.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
