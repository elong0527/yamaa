Following CDISC ADaM standards, use the provided ADSL, PLAN, and VS datasets to
create an ADVS dataset with one record per planned measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, VSSEQ, PARAMCD, ADT, AVAL, TRTSDT, HEIGHTBL

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
