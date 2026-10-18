Following CDISC ADaM standards, use the provided LB and SUPPLB datasets to
create an ADLB dataset with one record per laboratory record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, LBSEQ, QVAL, QVAL_NAMED

IDVARVAL is the record's LBSEQ padded with spaces to eight characters.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
