Following CDISC ADaM standards, use the provided LB and SUPPLB
datasets to create an ADLB dataset with one record per laboratory
record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, LBSEQ, LBTESTCD, VISITNUM, AVAL, ENDPOINT, EOTFL

AVAL is the numeric laboratory result.

ENDPOINT is Y when a supplemental endpoint qualifier with the same
study, subject, and sequence number marks the record; it has no value
otherwise. Only the protocol endpoint qualifier is read; other
qualifiers change nothing.

EOTFL is Y on one record per subject and test: the endpoint-marked
record when there is one, otherwise the record from the latest visit.
Every other record has no value. The endpoint qualifier decides even
when a later visit exists, and a record wins the flag even when its
result is missing.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
