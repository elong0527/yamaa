Following CDISC ADaM standards, use the provided LB and SUPPLB
datasets to create an ADLB dataset with one record per laboratory row.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, LBSEQ, QVAL, QVAL_NAMED

QVAL is the qualifier value from the supplemental record whose study,
subject, and padded sequence all match; it has no value when nothing
matches. QVAL_NAMED holds the same value from the same match, taken
again once every laboratory row exists; it has no value when nothing
matches.

The padding must be exact: a zero-padded key does not match the
space-padded form, and a supplemental record with no laboratory record
changes nothing. Rows in both sequence ranges use the same match.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
