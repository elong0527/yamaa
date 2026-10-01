Following CDISC SDTM standards, use the provided ODM dataset to create a
VS dataset with one record per blood pressure reading and one mean record
per test and visit.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSREPNUM,
VSORRES, VSORRESU, VSSTRESN, VSSTRESC, VSDRVFL

VISIT is VISIT 1 for study event SE.VISIT1 and VISIT 2 for SE.VISIT2.
VSTESTCD and VSTEST are SYSBP and Systolic Blood Pressure, or DIABP and
Diastolic Blood Pressure, from the collected item.

VSORRES is written as text; a whole number is written without a decimal
point. VSDRVFL is Y or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
