Following CDISC SDTM standards, use the provided ODM dataset to create a
VS dataset with one record per blood pressure reading and one mean record
per test and visit.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSREPNUM,
VSORRES, VSORRESU, VSSTRESN, VSSTRESC, VSDRVFL

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
