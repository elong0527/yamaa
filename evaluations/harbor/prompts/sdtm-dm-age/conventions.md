Following CDISC SDTM standards, use the provided ODM dataset to create
a DM dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, RFICDTC, BRTHDTC, AGE, AGEU

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

AGE is in whole calendar years. A February 29 birthday has its
anniversary on February 28 in a common year. AGEU is YEARS or has no
value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
