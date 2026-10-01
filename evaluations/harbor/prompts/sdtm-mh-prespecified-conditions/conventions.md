Following CDISC SDTM standards, use the provided ODM and MH_ITEMS datasets
to create an MH dataset with one record per checklist condition and per
volunteered free-text condition.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, MHSEQ, MHTERM, MHCAT, MHPRESP, MHOCCUR, MHSTAT

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

MHCAT is DISEASE-SPECIFIC HISTORY for checklist records and GENERAL HISTORY for
volunteered records. MHPRESP is Y or has no value. MHOCCUR is Y or N, or has no
value. MHSTAT is NOT DONE or has no value. MHSEQ numbers each subject's records
from 1: checklist conditions in the table's SORTORD order, then volunteered
conditions by visit (SCREENING before BASELINE) and form repeat.

Read the source datasets from /app/input and save the completed dataset as
/app/output/mh.csv.
