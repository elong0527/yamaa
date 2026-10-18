Following CDISC SDTM standards, use the provided ODM dataset to create a DM
dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SUBJID, SEX, AGE, ARM, ACTARM, ARMNRS

The dataset holds one record for each subject the input carries, whichever
items that subject has. STUDYID is the study OID and USUBJID is the subject
key from the input, each as given. SUBJID is the same value as USUBJID. SEX
is "M" when the collected sex is "Male" and "F" when it is "Female". SEX is
"U" when the sex is not collected, is blank, or is any value other than
"Male" or "Female". AGE is the age in whole years as collected and has no
value when no age was collected. ARM is the planned arm as collected and
has no value when no planned arm was collected. ACTARM is the same value as
ARM and has no value when ARM has no value. ARMNRS is "Not assigned to
treatment arm" when ARM has no value and has no value otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
