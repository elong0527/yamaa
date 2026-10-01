Following CDISC SDTM standards, use the provided VS_RAW, DM, TV, and SE
datasets to create a VS dataset with one record per collected vital-signs
result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSORRES, VSDTC, VISIT, VISITNUM,
EPOCH, VSDY

VISITNUM for an unscheduled visit after visit 2 reads 2.01. EPOCH is SCREENING,
TREATMENT, or FOLLOW-UP, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
