Following CDISC SDTM standards, use the provided ODM dataset to create a VS
dataset with one record per blood pressure reading.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VSSPID, VISIT, VSTESTCD, VSTEST, VSORRES,
VSORRESU, VSSTRESN, VSSTRESC

DOMAIN is "VS". STUDYID is the ODM study OID and USUBJID is the ODM subject
key, each as given. VISIT is the name of the visit the reading was collected
at, "VISIT 1" or "VISIT 2". VSTESTCD is "SYSBP" or "DIABP" and VSTEST is
"Systolic Blood Pressure" or "Diastolic Blood Pressure", the test code and
name of the collected item. Each reading taken gives one record, and VSSPID
is the reading's item-group repeat key, as text: it numbers the readings of
one test at one visit. A reading that was not taken, whether its record is
absent or its value is blank, gives no record, and the other readings keep
their numbers. VSORRES is the reading written as text from its numeric
value, so trailing zeros are dropped (a collected 82.50 reads 82.5).
VSSTRESN is the numeric reading. VSSTRESC is the numeric result written as
text, so it always agrees with VSSTRESN. VSORRESU is "mmHg" for every
record. No record for the mean of the readings is added. VSSEQ numbers each
subject's records from 1 by visit name, then by test code (DIABP before
SYSBP), then the readings in replicate order.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
