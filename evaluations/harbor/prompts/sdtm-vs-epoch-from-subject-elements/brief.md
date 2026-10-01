Following CDISC SDTM standards, use the provided VS_RAW and SE datasets to
create a VS dataset with one record per collected vital-signs result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSORRES, VSDTC, EPOCH

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
