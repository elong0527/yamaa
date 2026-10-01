Following CDISC SDTM standards, use the provided ODM dataset to create an
RS dataset with one record per subject and scheduled tumor assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, AVISITN, ADT, RSSEQ, RSTESTCD, RSTEST, RSSTRESC,
RSSTAT

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

AVISITN is the numeric order of the assessment, and RSSEQ numbers a subject's
response records in visit order, starting at 1. RSTESTCD is TRGRESP and RSTEST
is Timepoint Response on every record.

RSSTRESC is NE, CR, PR, PD, or SD, or has no value. RSSTAT is NOT DONE or has
no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/rs.csv.
