Following CDISC SDTM standards, use the provided ODM dataset to create
an AE dataset with one record per reported adverse event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, AEENDTC, AESEV, AESER

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

AESEQ numbers the subject's events from 1 in visit order (screening, then
baseline), then by visit repeat and item group repeat key.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
