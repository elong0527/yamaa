Following CDISC SDTM standards, use the provided ODM and VS_TESTS datasets to
create a VS dataset with one record per test from the vital signs form
collected at each visit.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSORRES, VSORRESU,
VSSTRESN, VSSTRESC, VSSTRESU, VSPOS, VSMETHOD, VSSTAT, VSREASND, VSDTC

DOMAIN is "VS". VSTESTCD is "SYSBP", "DIABP", "PULSE", "RESP", "TEMP",
or "WEIGHT". VSTEST is the test name VS_TESTS gives for the test code:
"Systolic Blood Pressure", "Diastolic Blood Pressure", "Pulse Rate",
"Respiratory Rate", "Temperature", or "Weight". VSSTAT is "NOT DONE"
or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
