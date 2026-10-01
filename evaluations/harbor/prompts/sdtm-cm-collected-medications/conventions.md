Following CDISC SDTM standards, use the provided ODM dataset to create
a CM dataset with one record per collected medication.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT, CMINDC, CMDOSE, CMDOSTXT,
CMDOSU, CMDOSFRQ, CMROUTE, CMSTDTC, CMENDTC, CMSTRTPT, CMSTTPT,
CMENRTPT, CMENTPT

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

CMSEQ is the medication repeat number within the subject. CMDOSFRQ maps the
collected frequency label to controlled terminology: Once daily becomes QD and
Twice daily becomes BID. CMROUTE maps the collected route label: By mouth
becomes ORAL. CMSTRTPT is BEFORE with CMSTTPT SCREENING, or both have no value.
CMENRTPT is ONGOING with CMENTPT END OF STUDY, or both have no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/cm.csv.
