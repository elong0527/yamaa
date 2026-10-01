Following CDISC SDTM standards, use the provided ODM and DM datasets to
create an AE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, AEENDTC, AESTDY, AEENDY

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

AESEQ is the form repeat number within the subject. AESTDTC and AEENDTC
keep the collected dates at the precision collected: a full date, a year
and month, or a year alone. Nothing is imputed, and a date with no part
collected has no value. AESTDY and AEENDY are the study days relative to
the subject reference start date in DM.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
