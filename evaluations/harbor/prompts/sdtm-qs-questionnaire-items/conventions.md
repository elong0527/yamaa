Following CDISC SDTM standards, use the provided ITEMS, NOTDONE, ODM, and
VISITS datasets to create a QS dataset with one record per questionnaire
item per subject per visit.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, QSSEQ, QSTESTCD, QSTEST, QSCAT, QSORRES, QSSTRESC,
QSSTRESN, QSSTAT, QSREASND, QSBLFL, QSDRVFL, VISITNUM, QSDTC

DOMAIN is "QS". QSCAT is "PHQ-9" on every record. QSTESTCD is
"PHQ901" through "PHQ909", "PHQ9T" on the total-score record, or
"QSALL" on the refused record; QSTEST is "Patient Health
Questionnaire 9 item total score" or "All Questionnaires". QSORRES
is "Not at all" for 0, "Several days" for 1, "More than half the
days" for 2, and "Nearly every day" for 3. QSSTAT is "NOT DONE" or
has no value; QSREASND is "LOGICALLY SKIPPED ITEM" or "SUBJECT
REFUSED", or has no value; QSBLFL and QSDRVFL are "Y" or have no
value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/qs.csv.
