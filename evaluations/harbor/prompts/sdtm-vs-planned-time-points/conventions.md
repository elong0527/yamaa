Following CDISC SDTM standards, use the provided ODM, DM, and VS_MAPPING
datasets to create a VS dataset with one record per collected vital-signs
result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTPT, VSTPTNUM, VSELTM, VSTESTCD,
VSORRES, VSORRESU, VSSTRESN, VSSTRESU, VSDTC, VSDY

VISIT is DAY 1 for study event SE.D1. VSTPT is PRE-DOSE, 30 MIN POST-DOSE, 1
H POST-DOSE, or 4 H POST-DOSE. VSTPTNUM is 1 through 4 in schedule order.
VSELTM is -PT15M before the dose, then PT30M, PT1H, and PT4H after it.

VSTESTCD is PULSE, SYSBP, or DIABP.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
