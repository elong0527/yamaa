Following CDISC ADaM standards, use the provided LB dataset to create an ADLBC
dataset with one record per subject per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISITN, AVAL, PREV_AVAL, CHG, PREV2

PARAMCD is "_ALB" on every record. AVISITN is the lab visit number,
and AVAL is the numeric albumin result for the visit. A record appears
only for an albumin result that has a numeric value; a record for
another test, or an albumin record with no numeric result, has no
record in the output. PREV_AVAL is the AVAL of the visit just before
in AVISITN order for the same subject, and has no value on the
subject's first visit. CHG is AVAL minus PREV_AVAL, the change since
that visit, and has no value whenever PREV_AVAL has no value. CHG is
the change from the previous visit, not a change from baseline, and a
change of zero is shown as 0, not left empty. PREV2 is the CHG of the
visit just before, taken from that visit's completed CHG, and has no
value until two earlier visits carry results. A record with no
numeric result shortens the chain: PREV_AVAL, CHG, and PREV2 are read
against the latest earlier visit that has a result.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlbc.csv.
