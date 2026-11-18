Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per collected laboratory record, plus one average
record per subject and parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, LBSEQ, PARAMCD, AVAL, DTYPE

PARAMCD is the collected test code. On a collected record, LBSEQ is the
collected sequence number, AVAL is the collected numeric result and has no
value when no usable result was collected, and DTYPE has no value. The
average record of a subject and parameter has no LBSEQ, AVAL is the mean of
the subject's non-missing results for the parameter, and DTYPE is "AVERAGE".
A subject with no non-missing result for a parameter has no average record.
AVAL is not rounded. ASEQ numbers each subject's records from 1: by PARAMCD,
then the collected records in LBSEQ order, then the average record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
