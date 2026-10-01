Following CDISC SDTM standards, use the provided ODM dataset to create a
VS dataset with one record per blood pressure reading and one mean record
per test and visit.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSREPNUM,
VSORRES, VSORRESU, VSSTRESN, VSSTRESC, VSDRVFL

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

VISIT is VISIT 1 for study event SE.VISIT1 and VISIT 2 for SE.VISIT2. VSTESTCD
and VSTEST are SYSBP and Systolic Blood Pressure, or DIABP and Diastolic Blood
Pressure, from the collected item. VSSEQ numbers every record of a subject from
1, by visit name, then test code (DIABP before SYSBP): each test's readings in
replicate order, then its mean record. VSREPNUM is the item-group repeat key
numbering the readings of one test at one visit, or has no value.

VSORRES is written as text; a whole number is written without a decimal point.
VSORRESU is mmHg for every record. VSSTRESC is the same value written as text,
so it always agrees with the numeric result. VSDRVFL is Y or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
