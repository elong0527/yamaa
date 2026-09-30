Following CDISC SDTM standards, use the provided ODM, VISITS, NOTDONE, and
ITEMS datasets to create a QS dataset with one record per questionnaire
item at each visit, plus a total-score record for each fully answered
questionnaire.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, QSSEQ, QSTESTCD, QSTEST, QSCAT, QSORRES, QSSTRESC,
QSSTRESN, QSSTAT, QSREASND, QSBLFL, QSDRVFL, VISITNUM, QSDTC

QSSEQ numbers each subject's records by visit, then by test short name, so
the total-score record follows the nine items and the
refused-questionnaire record comes last. QSTESTCD is PHQ901 through PHQ909
for the nine items, PHQ9T for the total-score record, and QSALL for the
refused questionnaire. QSTEST is the item text from the item table; the
total record reads "Patient Health Questionnaire 9 item total score" and
the refused record reads "All Questionnaires". QSCAT is always PHQ-9.

QSORRES is the response text matching the circled score: 0 is "Not at
all", 1 is "Several days", 2 is "More than half the days", and 3 is
"Nearly every day". It has no value on the total-score record because the
score is computed, not collected. QSSTRESC is the collected score as text,
or the total as text on the total record, written without a decimal point;
QSSTRESN is the same value as a number. The total is the sum of the nine
item scores and appears only for a visit where all nine items were
answered.

QSSTAT is NOT DONE on the skipped-item record and on the
refused-questionnaire record; QSREASND says why: LOGICALLY SKIPPED ITEM or
SUBJECT REFUSED. Records with QSSTAT NOT DONE have no value for QSORRES,
QSSTRESC, and QSSTRESN. QSDRVFL is Y on the total-score record only.
QSBLFL is Y on every record from the first visit. VISITNUM and QSDTC are
the visit number and collection date from the visit table.

Read the source datasets from /app/input and save the completed dataset as
/app/output/qs.csv.
