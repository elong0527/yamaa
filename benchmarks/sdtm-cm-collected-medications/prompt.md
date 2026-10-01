Following CDISC SDTM standards, use the provided ODM dataset to create
a CM dataset with one record per collected medication.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT, CMINDC, CMDOSE, CMDOSTXT,
CMDOSU, CMDOSFRQ, CMROUTE, CMSTDTC, CMENDTC, CMSTRTPT, CMSTTPT,
CMENRTPT, CMENTPT

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

CMSEQ is the medication repeat number within the subject. CMTRT and
CMINDC keep the reported name and indication exactly as written. CMDOSE
holds the collected dose when it is a plain number, and has no value
otherwise. CMDOSTXT holds the collected dose when it is not a plain
number, such as a range like 50-75, and has no value when CMDOSE carries
the dose. CMDOSU is the unit of the collected dose. CMDOSFRQ maps the
collected frequency label to controlled terminology: Once daily becomes
QD and Twice daily becomes BID. CMROUTE maps the collected route label:
By mouth becomes ORAL. CMSTDTC keeps the start as collected, with a
partial date at its collected precision. CMENDTC keeps the end as
collected, and has no value when no end date was collected. CMSTRTPT is
BEFORE with CMSTTPT SCREENING when the taken-before-study box is
checked, and both have no value otherwise. CMENRTPT is ONGOING with
CMENTPT END OF STUDY when the ongoing box is checked, and both have no
value otherwise. An ongoing medication has no end date.

Read the source datasets from /app/input and save the completed dataset as
/app/output/cm.csv.
