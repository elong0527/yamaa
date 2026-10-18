Following CDISC ADaM standards, use the provided LB dataset to create an ADLBC
dataset with one record per subject per parameter per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISITN, AVAL, PREV_AVAL, CHG

- The dataset holds rows only for LB records with test code ALB or BILI
  and a numeric result; records with another test code or no numeric
  result have no row.
- PARAMCD is "_ALB" for an albumin (ALB) record and "_BILI" for a
  bilirubin (BILI) record.
- AVISITN is the lab visit number, and AVAL is the numeric lab result
  for the visit.
- PREV_AVAL is the AVAL of the latest earlier visit with a result for
  the same subject and parameter in AVISITN order, so an "_ALB" row
  never takes a value from a "_BILI" row.
- PREV_AVAL has no value for the earliest visit with a result for a
  subject and parameter.
- CHG is AVAL - PREV_AVAL, the change since the previous visit rather
  than a change from baseline; it has no value whenever PREV_AVAL has
  no value, and equal consecutive values give CHG 0.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlbc.csv.
