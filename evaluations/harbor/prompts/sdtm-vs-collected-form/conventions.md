Following CDISC SDTM standards, use the provided ODM and TESTS datasets to
create a VS dataset with one record per test from the wide vital signs
form collected at each visit.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSORRES, VSORRESU,
VSSTRESN, VSSTRESC, VSSTRESU, VSPOS, VSMETHOD, VSSTAT, VSREASND, VSDTC

VSTESTCD is SYSBP, DIABP, PULSE, RESP, TEMP, or WEIGHT; VSTEST is Systolic
Blood Pressure, Diastolic Blood Pressure, Pulse Rate, Respiratory Rate,
Temperature, or Weight.

VSORRESU is mmHg, beats/min, breaths/min, C, or kg, or has no value. In
VSSTRESC a whole number is written without a decimal point. VSPOS is SITTING
or has no value. VSMETHOD is ORAL or has no value. VSSTAT is NOT DONE or has
no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
