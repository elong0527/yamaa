Following CDISC SDTM standards, use the provided ODM dataset to create
a CM dataset with one record per reported medication course.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT, CMSTDTC, CMENDTC, CMROUTE, CMINDC

Each form repeat within a visit repeat is a separate course, so a form
repeat number reused at a later visit starts a separate course, and a
medication reported in two courses gives two records. Only CM forms are
read, and a course without a reported treatment name gives no record.
CMTRT keeps the medication name exactly as reported. CMSTDTC and
CMENDTC come from the same course; an ongoing course has no value for
the end date. CMROUTE and CMINDC keep that course's route and
indication. CMSEQ numbers the subject's courses from 1 by visit
(screening before baseline), visit repeat, and form repeat.

Read the source datasets from /app/input and save the completed dataset as
/app/output/cm.csv.
