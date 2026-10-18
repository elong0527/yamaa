Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per collected laboratory record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, LBSEQ, PARAMCD, AVAL, AVALMEAN

PARAMCD is the record's collected test code. AVAL is the collected numeric
result for the record, and has no value when no usable result was collected.
AVALMEAN is the mean of the subject's non-missing AVAL values for the
record's parameter, and the same mean is repeated on every record for that
subject and parameter, including a record whose own AVAL has no value. When
the subject has no non-missing AVAL for the parameter, AVALMEAN has no
value; it is never zero. AVALMEAN is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
