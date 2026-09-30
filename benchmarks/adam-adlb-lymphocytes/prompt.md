Following CDISC ADaM standards, use the provided ADLB_RAW dataset to
create an ADLB dataset with one record per subject per visit per
parameter.

The output dataset should contain the following columns in this order:
USUBJID, PARAMCD, AVAL, PARAM, VISIT, DTYPE

Keep every collected record unchanged, with DTYPE empty.

Add one "LYMPH" record per subject and visit that has both a white
blood cell count (WBC) and a lymphocyte fraction (LYMLE) result and
no LYMPH record yet. Its value is the WBC count times the LYMLE
fraction for the same subject and visit, its parameter name is
"Lymphocytes Abs (10^9/L)", and DTYPE is CALCULATION.

A subject and visit with only one of the two results, or one that
already holds a LYMPH record, gains none.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
