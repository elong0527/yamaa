Following CDISC ADaM standards, use the provided PP and ADSL datasets to
create an ADPP dataset with one record per subject per parameter per
analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, AVALU

Keep every PP record, including a parameter that was not calculated:
PARAMCD and PARAM are PPTESTCD and PPTEST, AVISIT is the collected VISIT,
AVAL is PPSTRESN and AVALU is PPSTRESU. Add per subject the accumulation
ratios "ARAUC" with PARAM "Accumulation Ratio AUCTAU" and "ARCMAX" with
PARAM "Accumulation Ratio Cmax": the subject's steady-state AUCTAU or CMAX
divided by the same subject's first-dose value. The first-dose profile is
the "DAY 1" profile. The steady-state profile is the one on the last
planned dosing day of the subject's planned arm (ARMCD): arm "A" is dosed
once daily on days 1 to 7, so its steady-state profile is "DAY 7", and arm
"B" on days 1 to 5, so "DAY 5". A profile on any other day enters neither
value. A ratio record exists only when both values are present. It has the
AVISIT of the steady-state profile and AVALU "RATIO", and its AVAL is not
rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
