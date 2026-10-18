Following CDISC SDTM standards, use the provided ITEMS, NOTDONE, ODM, and
VISITS datasets to create a QS dataset with one record per questionnaire
item per subject per visit.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, QSSEQ, QSTESTCD, QSTEST, QSCAT, QSORRES, QSSTRESC,
QSSTRESN, QSSTAT, QSREASND, QSBLFL, QSDRVFL, VISITNUM, QSDTC

DOMAIN is "QS".
STUDYID is the study OID in ODM and USUBJID is the subject key in ODM,
each as given.
Each collected item gives one record; QSTESTCD is the item's short name
in ITEMS, "PHQ901" through "PHQ909", and QSTEST is the item text in
ITEMS.
QSCAT is "PHQ-9" on every record.
QSORRES is the response text matching the circled score: "Not at all"
for 0, "Several days" for 1, "More than half the days" for 2, and
"Nearly every day" for 3.
QSSTRESC is the collected score as text, and QSSTRESN is the collected
score as a number.
VISITNUM is the visit number VISITS gives for the subject's visit, and
QSDTC is the collection date VISITS gives for it.
A visit where all nine items were answered also gives a total-score
record, with QSTESTCD "PHQ9T" and QSTEST "Patient Health Questionnaire
9 item total score".
On the total-score record, QSSTRESC is the sum of the nine item scores
as text and QSSTRESN is that sum as a number.
The total-score record's QSORRES has no value, because the total is
computed, not collected.
A visit with a skipped or a refused item gives no total-score record.
QSDRVFL is "Y" on the total-score record only, and has no value on
every other record.
NOTDONE declares what was not collected: a row naming an item is that
item's own record, and a row naming no item stands for the whole
questionnaire.
The skipped item keeps its record, with its item's QSTESTCD and QSTEST,
QSSTAT "NOT DONE", and QSREASND "LOGICALLY SKIPPED ITEM".
A refused questionnaire keeps one record instead of nine, with
QSTESTCD "QSALL", QSTEST "All Questionnaires", QSSTAT "NOT DONE", and
QSREASND "SUBJECT REFUSED".
A record with QSSTAT "NOT DONE" carries no result: QSORRES, QSSTRESC,
and QSSTRESN have no value.
QSSTAT and QSREASND have no value on every other record.
QSBLFL is "Y" on every record whose VISITNUM is 1, and has no value on
every other record.
QSSEQ numbers a subject's records from 1 by visit number, then by
QSTESTCD.
The total-score record follows the nine item records of its visit, and
the refused-questionnaire record is the last record of its visit.

Read the source datasets from /app/input and save the completed dataset as
/app/output/qs.csv.
