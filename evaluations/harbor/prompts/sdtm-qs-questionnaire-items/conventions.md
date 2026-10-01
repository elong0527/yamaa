Following CDISC SDTM standards, use the provided ODM, VISITS, NOTDONE, and
ITEMS datasets to create a QS dataset with one record per questionnaire
item at each visit, plus a total-score record for each fully answered
questionnaire.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, QSSEQ, QSTESTCD, QSTEST, QSCAT, QSORRES, QSSTRESC,
QSSTRESN, QSSTAT, QSREASND, QSBLFL, QSDRVFL, VISITNUM, QSDTC

QSTESTCD is PHQ901 through PHQ909 for the nine items, PHQ9T for the total-
score record, and QSALL for the refused questionnaire; QSTEST is the item
text from the item table, "Patient Health Questionnaire 9 item total score",
or "All Questionnaires". QSCAT is always PHQ-9. QSORRES is "Not at all",
"Several days", "More than half the days", or "Nearly every day" for a
circled score of 0 through 3. QSSTRESC is written without a decimal point.
QSSTAT is NOT DONE or has no value, with QSREASND LOGICALLY SKIPPED ITEM or
SUBJECT REFUSED. QSDRVFL and QSBLFL are Y or have no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/qs.csv.
