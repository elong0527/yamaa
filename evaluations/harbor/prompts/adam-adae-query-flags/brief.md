Following CDISC ADaM standards, use the provided AE and QUERY datasets to
create an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AEDECOD, SMQ01NAM, SMQ01CD, SMQ01SC,
SMQ02NAM, SMQ02CD, SMQ02SC, CQ01NAM

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
