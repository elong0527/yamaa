Following CDISC SDTM standards, use the provided ODM dataset to create
a DM dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SUBJID, SEX, AGE, ARM, ACTARM, ARMNRS

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

There is one record for each subject the extract carries, and SUBJID
repeats USUBJID. SEX codes the recorded sex: M for Male and F for
Female, and U when missing, blank, not collected at all, or any other
value. AGE is the age in whole years as collected, and has no value
when missing. ARM is the planned arm as collected, with no value when
none was collected. ACTARM always equals the planned arm, so it has no
value when ARM does. ARMNRS is Not assigned to treatment arm when ARM
has no value, and has no value otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
