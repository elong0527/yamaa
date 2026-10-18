Following CDISC ADaM standards, use the provided LB and SUPPLB datasets to
create an ADLB dataset with one record per laboratory record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, LBSEQ, LBTESTCD, VISITNUM, AVAL, ENDPOINT, EOTFL

AVAL is the numeric laboratory result. ENDPOINT is "Y" when SUPPLB holds a
qualifier with QNAM "ENDPOINT" for the same study, subject, and sequence
number as the record; ENDPOINT has no value otherwise. In SUPPLB the
sequence number is eight-character, space-padded text. EOTFL is "Y" on one
record per subject and test: the endpoint-marked record when there is one,
otherwise the record from the latest visit. Every other record has no
value for EOTFL. The endpoint mark decides the choice even when a later
visit exists, so the flag can sit before the subject's last visit. A record
wins the flag even when its result is missing, and a subject and test with
a single record still gets the flag. Other qualifiers on the same record,
and endpoint qualifiers for records the laboratory data does not hold,
change nothing.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
