Following CDISC ADaM standards, use the provided LB and SUPPLB datasets to
create an ADLB dataset with one record per laboratory record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, LBSEQ, QVAL, QVAL_NAMED

QVAL is the qualifier value of the SUPPLB record whose STUDYID and
USUBJID match the laboratory record and whose IDVARVAL is the record's
LBSEQ padded with spaces to eight characters. QVAL has no value when no
SUPPLB record matches. QVAL_NAMED holds the same value from the same
match and has no value when no SUPPLB record matches. The padding is
exact: an IDVARVAL padded with zeros does not match. A SUPPLB record
that matches no laboratory record changes nothing. Rows with an LBSEQ
below 200 and rows with an LBSEQ of 200 or more use the same match.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
