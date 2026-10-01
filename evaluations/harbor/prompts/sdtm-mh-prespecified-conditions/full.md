Following CDISC SDTM standards, use the provided ODM and MH_ITEMS datasets
to create an MH dataset with one record per checklist condition and per
volunteered free-text condition.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, MHSEQ, MHTERM, MHCAT, MHPRESP, MHOCCUR, MHSTAT

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

MHTERM is the checklist condition from the item-definition table, or the
volunteered text exactly as reported. MHCAT is DISEASE-SPECIFIC HISTORY
for checklist records and GENERAL HISTORY for volunteered records. MHPRESP
is Y for checklist records and has no value for volunteered records.
MHOCCUR is Y or N for an answered checklist condition and has no value
otherwise. MHSTAT is NOT DONE for an unanswered checklist condition and
has no value otherwise. MHSEQ numbers each subject's records from 1:
checklist conditions in the table's SORTORD order, then volunteered
conditions by visit (SCREENING before BASELINE) and form repeat.

An unanswered checklist item still has an extract record. A question
entirely absent from the extract is not assumed to have been asked. Each
volunteered condition is its own MH record, even when its repeat number is
reused at another visit.

Read the source datasets from /app/input and save the completed dataset as
/app/output/mh.csv.
