Following CDISC SDTM standards, use the provided ODM dataset to create a CM
dataset with one record per reported medication course.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT, CMSTDTC, CMENDTC, CMROUTE, CMINDC

DOMAIN is "CM". STUDYID is the study OID and USUBJID is the subject key of
the ODM data, each as given. Each ODM row holds one collected item, and the
items that share a subject, a visit, a visit repeat number, and a form
repeat number make up one medication course. Only items from the IG.CM form
contribute: items from other forms contribute no CM record. A course with
no reported treatment name contributes no CM record. CMTRT is the
medication name exactly as reported for the course. CMSTDTC is the start
date and CMENDTC the end date reported for that same course, and an ongoing
course has no value for CMENDTC. CMROUTE and CMINDC are the route and
indication reported for that same course. A form repeat number reused at a
later visit starts a separate course, and a medication reported in two
courses gives two records. CMSEQ numbers a subject's courses from 1, in
visit order with screening before baseline, then by visit repeat number,
then by form repeat number.

Read the source datasets from /app/input and save the completed dataset as
/app/output/cm.csv.
