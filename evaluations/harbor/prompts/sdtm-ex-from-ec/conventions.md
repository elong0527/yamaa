Following CDISC SDTM standards, use the provided ODM and KITLIST
datasets to create an EX dataset with one record per administered
exposure.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU, EXSTDTC, EXENDTC

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

EXSEQ numbers the administrations within the subject by start date and then by
the form repeat number within the visit. EXDOSU is mg, except the AUC target
keeps AUC.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ex.csv.
