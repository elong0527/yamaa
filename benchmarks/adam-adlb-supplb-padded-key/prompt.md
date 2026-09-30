Following CDISC ADaM standards, use the provided LB and SUPPLB
datasets to create an ADLB dataset with one record per laboratory row.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, LBSEQ, QVAL, QVAL_NAMED

QVAL is the qualifier value from the supplemental record with the same
study and subject whose IDVARVAL is the laboratory sequence number
written as eight characters, right-aligned and padded with spaces; it
has no value when no such record exists. The padding must be exact: a
zero-padded IDVARVAL does not match, and a supplemental record with no
laboratory record adds no row. QVAL_NAMED repeats QVAL.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
