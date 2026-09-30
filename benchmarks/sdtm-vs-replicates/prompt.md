Following CDISC SDTM standards, use the provided ODM dataset to create a
VS dataset with one record per blood pressure reading and one mean record
per test and visit.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSREPNUM,
VSORRES, VSORRESU, VSSTRESN, VSSTRESC, VSDRVFL

VISIT is VISIT 1 for study event SE.VISIT1 and VISIT 2 for SE.VISIT2.
VSTESTCD and VSTEST are SYSBP and Systolic Blood Pressure, or DIABP and
Diastolic Blood Pressure, from the collected item. VSSEQ numbers every
record of a subject from 1, by visit name, then test code (DIABP before
SYSBP): each test's readings in replicate order, then its mean record.
VSREPNUM is the item-group repeat key numbering the readings of one test
at one visit, and has no value on the mean record.

VSORRES is the reading written as text, and the mean on the mean record;
a whole number is written without a decimal point. VSORRESU is mmHg for
every record. VSSTRESN is the reading itself on a reading record, and the
mean of the readings actually taken on the mean record. VSSTRESC is the
same value written as text, so it always agrees with the numeric result.
VSDRVFL marks the mean record with Y and has no value on the reading
records.

A reading that was not taken, whether its record is absent or its value is
blank, gives no reading record, and the visit mean is taken over the
readings present.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
