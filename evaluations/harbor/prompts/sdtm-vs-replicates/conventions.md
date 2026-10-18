Following CDISC SDTM standards, use the provided ODM dataset to create a VS
dataset with one record per reading, plus one record per test and visit
holding the mean of the readings actually taken.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSREPNUM, VSORRES,
VSORRESU, VSSTRESN, VSSTRESC, VSDRVFL

DOMAIN is "VS". VISIT is "VISIT 1" or "VISIT 2". VSTESTCD is "SYSBP" or
"DIABP" and VSTEST is "Systolic Blood Pressure" or "Diastolic Blood
Pressure". VSORRES is the reading written as text from its numeric value,
so trailing zeros are dropped (a collected 82.50 reads 82.5). VSDRVFL is
"Y" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
