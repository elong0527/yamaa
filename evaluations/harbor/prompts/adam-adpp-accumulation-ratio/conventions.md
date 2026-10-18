Following CDISC ADaM standards, use the provided ADSL and PP datasets to create
an ADPP dataset with one record per subject, parameter and visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, AVALU

A ratio record for the area under the curve over the dosing interval has
PARAMCD "ARAUC" and PARAM "Accumulation Ratio AUCTAU"; a ratio record for
the maximum concentration has PARAMCD "ARCMAX" and PARAM "Accumulation
Ratio Cmax". AVAL on a ratio record is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
