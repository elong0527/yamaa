Following CDISC SDTM standards, use the provided ODM, DM, and VS_MAPPING
datasets to create a VS dataset with one record per collected vital-signs
result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTPT, VSTPTNUM, VSELTM, VSTESTCD,
VSORRES, VSORRESU, VSSTRESN, VSSTRESU, VSDTC, VSDY

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
