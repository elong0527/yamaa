Following CDISC SDTM standards, use the provided DM, SE, TV, and VS_RAW
datasets to create a VS dataset with one record per collected
vital-signs result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VSTESTCD, VSORRES, VSDTC, VISIT, VISITNUM,
EPOCH, VSDY

DOMAIN is "VS". EPOCH is "SCREENING", "TREATMENT", or "FOLLOW-UP", or has
no value. VSDY is the study day of the collection date, counted from the
subject's reference start date in DM.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
