Following CDISC SDTM standards, use the provided ODM and TESTS datasets to
create a VS dataset with one record per test from the wide vital signs
form collected at each visit.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSORRES, VSORRESU,
VSSTRESN, VSSTRESC, VSSTRESU, VSPOS, VSMETHOD, VSSTAT, VSREASND, VSDTC

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
