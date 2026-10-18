Following CDISC ADaM standards, use the provided CE dataset to create an ADCE
dataset with one record per clinical event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CESEQ, CETERM, ASTDT, ASEV, ASEVN, ATOXGRN, AOCCFL

ASEV is the severity collected for the event, and has no value when no
severity was collected. ASEVN is 1 for "MILD", 2 for "MODERATE", and 3 for
"SEVERE", and has no value when no severity was collected. ATOXGRN is the
same grade as ASEVN, and has no value when no severity was collected.
AOCCFL is "Y" on the one event of each subject that has the greatest
ATOXGRN, and has no value on every other event. When several of a subject's
events share the greatest grade, the flagged event is the one with the
earliest ASTDT, and the lowest CESEQ where the start dates also tie. An
event with no grade is never flagged, so a subject whose events all have
no grade has no flagged event.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adce.csv.
