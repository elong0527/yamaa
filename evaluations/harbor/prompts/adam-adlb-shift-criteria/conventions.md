Following CDISC ADaM standards, use the provided ADLB_RAW dataset to
create an ADLB dataset with one record per laboratory record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ASEQ, AVISIT, AVAL, ANRLO, ANRHI,
ANRIND, ABLFL, BASE, BNRIND, SHIFT1, R2BASE, CRIT1, CRIT1FL,
CRIT2, CRIT2FL

ANRIND is LOW, HIGH, or NORMAL, or has no value.

SHIFT1 joins the baseline mark and the record's own mark, baseline first, so a
result that stayed normal reads "NORMAL to NORMAL".

R2BASE is not rounded.

CRIT1 is "Result greater than 3 x ULN" or has no value. CRIT1FL is Y or N, or
has no value.

CRIT2 is "Result less than LLN" or has no value. CRIT2FL is Y or N, or
has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
