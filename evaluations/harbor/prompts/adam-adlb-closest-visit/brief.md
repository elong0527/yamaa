Following CDISC ADaM standards, use the provided ADLB_RAW dataset to
create an ADLB dataset with one record per laboratory measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, LBSEQ, ADT, ADY, AVAL, AVISIT, AWTARGET,
ADIST, ANL01FL

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
