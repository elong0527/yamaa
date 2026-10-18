Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per laboratory result.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, LBSEQ, PARAMCD, AVALC, AVAL, AVALU, DTYPE

PARAMCD is the laboratory test code from LBTESTCD. AVALC is the
reported result text from LBSTRESC, and has no value when no result was
reported. AVAL is the numeric value of the reported result under the
study's one-unit policy: one less than the limit for a result reported
with a strict "<", one more than the limit for a result reported with a
strict ">", and the limit itself for a result reported with "<=" or
">=". An ordinary number, or a result reported with "=", keeps its
numeric value, and a missing result has no AVAL, which is distinct from
zero. AVALU is the standard unit from LBSTRESU, copies/mL. DTYPE is
"IMPUTATION" when the reported result is an inequality, reported with
"<", ">", "<=", or ">=", and has no value otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
