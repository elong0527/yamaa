Following CDISC SDTM standards, use the provided ODM, VISITS, NOTDONE, and
ITEMS datasets to create a QS dataset with one record per questionnaire
item at each visit, plus a total-score record for each fully answered
questionnaire.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, QSSEQ, QSTESTCD, QSTEST, QSCAT, QSORRES, QSSTRESC,
QSSTRESN, QSSTAT, QSREASND, QSBLFL, QSDRVFL, VISITNUM, QSDTC

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

QSSEQ numbers each subject's records by visit, then by test short name, so the
total-score record follows the nine items and the refused-questionnaire record
comes last. QSTESTCD is PHQ901 through PHQ909 for the nine items, PHQ9T for the
total-score record, and QSALL for the refused questionnaire. QSTEST is the item
text from the item table; the total record reads "Patient Health Questionnaire
9 item total score" and the refused record reads "All Questionnaires". QSCAT is
always PHQ-9.

QSORRES is the response text matching the circled score: 0 is "Not at all", 1
is "Several days", 2 is "More than half the days", and 3 is "Nearly every day".
QSSTRESC is written without a decimal point.

QSSTAT is NOT DONE or has no value; QSREASND says why: LOGICALLY SKIPPED ITEM
or SUBJECT REFUSED. QSDRVFL is Y or has no value. QSBLFL is Y or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/qs.csv.
