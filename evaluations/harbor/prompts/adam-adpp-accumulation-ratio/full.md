Following CDISC ADaM standards, use the provided ADSL and PP datasets to create
an ADPP dataset with one record per subject, parameter and visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, AVALU

- Every record of PP appears in the output as a collected record.
- On a collected record PARAMCD is the collected PPTESTCD, PARAM is the
  collected PPTEST, and AVISIT is the collected visit.
- On a collected record AVAL is the collected standard result and AVALU is
  the collected standard unit. AVAL has no value when the parameter was not
  calculated, and that record stays in the output with no value.
- A ratio record for the area under the curve over the dosing interval has
  PARAMCD "ARAUC" and PARAM "Accumulation Ratio AUCTAU"; a ratio record for
  the maximum concentration has PARAMCD "ARCMAX" and PARAM "Accumulation
  Ratio Cmax".
- The steady-state profile is the profile on the last planned dosing day of
  the subject's planned arm in ADSL, "DAY 7" for arm A and "DAY 5" for arm
  B; the first-dose profile is the "DAY 1" profile.
- On a ratio record AVISIT is the visit of the steady-state profile.
- On a ratio record AVAL is the steady-state AUCTAU or CMAX value divided
  by the day 1 value of the same parameter for the same subject. AVAL on a
  ratio record is not rounded and may be below 1.
- On a ratio record AVALU is "RATIO".
- A profile on any other day, such as an interim profile during dosing or
  the last profile of a subject who stopped dosing early, enters neither
  value.
- A ratio record exists only when both the steady-state value and the day 1
  value are present.
- When the day 1 value is zero, the ratio has no value and no ratio record
  exists for that parameter.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
