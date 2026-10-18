Following CDISC SDTM standards, use the provided ODM dataset to create a VS
dataset with one record per reading, plus one record per test and visit
holding the mean of the readings actually taken.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSREPNUM, VSORRES,
VSORRESU, VSSTRESN, VSSTRESC, VSDRVFL

DOMAIN is "VS". STUDYID is the ODM study OID and USUBJID is the ODM subject
key, each as given. VISIT is the name of the visit the reading was collected
at, "VISIT 1" or "VISIT 2". VSTESTCD is "SYSBP" or "DIABP" and VSTEST is
"Systolic Blood Pressure" or "Diastolic Blood Pressure", the test code and
name of the collected item. Each reading taken gives one record, and
VSREPNUM is the reading's item-group repeat key: it numbers the readings of
one test at one visit. A reading that was not taken, whether its record is
absent or its value is blank, gives no reading record. VSORRES is the
reading written as text from its numeric value, so trailing zeros are
dropped (a collected 82.50 reads 82.5). VSSTRESN is the reading itself on a
reading record. VSSTRESC is the numeric result written as text, so it
always agrees with VSSTRESN. VSORRESU is "mmHg" for every record. Each test
at each visit also gives one derived record holding the mean of the
readings actually taken at that visit: on that record VSSTRESN is the mean,
VSORRES carries the mean written as text, and VSREPNUM has no value.
VSDRVFL is "Y" on the derived record and has no value on the reading
records. VSSEQ numbers each subject's records from 1 by visit name, then
by test code (DIABP before SYSBP): each test's readings in replicate order,
then its mean record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
