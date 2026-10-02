Following CDISC ADaM standards, use the provided ADLB_RAW dataset to
create an ADLB dataset with one record per laboratory record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ASEQ, AVISIT, AVAL, ANRLO, ANRHI,
ANRIND, ABLFL, BASE, BNRIND, SHIFT1, R2BASE, CRIT1, CRIT1FL,
CRIT2, CRIT2FL

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
