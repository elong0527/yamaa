Following CDISC SDTM standards, use the provided ODM, DM, and VS_MAPPING
datasets to create a VS dataset with one record per collected vital-signs
result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTPT, VSTPTNUM, VSELTM, VSTESTCD,
VSORRES, VSORRESU, VSSTRESN, VSSTRESU, VSDTC, VSDY

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

VSSEQ numbers the records per subject from 1 in schedule order, then pulse,
systolic, and diastolic blood pressure within each time point. VISIT is DAY 1
for study event SE.D1. VSTPT is PRE-DOSE, 30 MIN POST-DOSE, 1 H POST-DOSE, or 4
H POST-DOSE. VSTPTNUM is 1 through 4 in schedule order. VSELTM is -PT15M before
the dose, then PT30M, PT1H, and PT4H after it.

VSTESTCD is PULSE, SYSBP, or DIABP. VSORRES is the result in original units,
and VSORRESU the original unit (beats/min or mmHg). VSSTRESN is the numeric
result in standard units; pulse and blood pressure were collected in standard
units. VSSTRESU is the standard unit, carried from the original unit.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
