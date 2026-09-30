Following CDISC ADaM standards, use the provided LB dataset to create
an ADLB dataset with one record per collected laboratory record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, LBSEQ, PARAMCD, AVAL, AVALMEAN

AVAL is the collected numeric result for the record; it has no value
when no usable result was collected.

AVALMEAN is the mean of the subject's collected values for the
parameter, repeated on every record for that subject and parameter,
including a record whose own value is missing. It has no value, never
zero, when the subject has no collected value for the parameter. The
mean is taken separately for each test code.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
