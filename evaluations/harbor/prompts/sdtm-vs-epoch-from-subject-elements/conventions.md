Following CDISC SDTM standards, use the provided SE and VS_RAW datasets to
create a VS dataset with one record per vital signs measurement.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSORRES, VSDTC, EPOCH

DOMAIN is "VS". EPOCH has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
