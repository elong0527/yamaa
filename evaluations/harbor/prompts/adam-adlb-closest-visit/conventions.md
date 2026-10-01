Following CDISC ADaM standards, use the provided ADLB_RAW dataset to
create an ADLB dataset with one record per laboratory measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, LBSEQ, ADT, ADY, AVAL, AVISIT, AWTARGET,
ADIST, ANL01FL

AVISIT is WEEK 2 or has no value. AWTARGET is the visit's target day, day
15.

ANL01FL is Y or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
