Following CDISC SDTM standards, use the provided DM, ODM, and VS_MAPPING
datasets to create a VS dataset with one record per collected vital-signs
result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTPT, VSTPTNUM, VSELTM, VSTESTCD,
VSORRES, VSORRESU, VSSTRESN, VSSTRESU, VSDTC, VSDY

DOMAIN is "VS". VISIT is "DAY 1" on every record. VSTPT is
"PRE-DOSE", "30 MIN POST-DOSE", "1 H POST-DOSE", or "4 H POST-DOSE".
VSTPTNUM is 1, 2, 3, or 4 in schedule order. VSELTM is in ISO 8601
duration form: "-PT15M", "PT30M", "PT1H", or "PT4H". VSTESTCD is
"PULSE", "SYSBP", or "DIABP". VSDY is the study day of the collection
datetime, counted from the subject's reference start date in DM, or
has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
