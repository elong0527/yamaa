Following CDISC SDTM standards, use the provided ODM, VISITS, NOTDONE, and
ITEMS datasets to create a QS dataset with one record per questionnaire
item at each visit, plus a total-score record for each fully answered
questionnaire.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, QSSEQ, QSTESTCD, QSTEST, QSCAT, QSORRES, QSSTRESC,
QSSTRESN, QSSTAT, QSREASND, QSBLFL, QSDRVFL, VISITNUM, QSDTC

Read the source datasets from /app/input and save the completed dataset as
/app/output/qs.csv.
