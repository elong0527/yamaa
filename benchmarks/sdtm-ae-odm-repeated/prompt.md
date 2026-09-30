Following CDISC SDTM standards, use the provided ODM dataset to create
an AE dataset with one record per reported adverse event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, AEENDTC, AESEV, AESER

Each form occurrence is its own event: items sharing subject, visit,
visit repeat, form, and form repeat belong to one event, so a form
repeat number reused at a later visit, or the same term reported in
two occurrences, still gives separate records. Only AE forms are read,
and an occurrence without a reported term gives no record. AETERM keeps
the free-text term exactly as reported. AESTDTC and AEENDTC come from
the same occurrence; a missing end date has no value. AESEV and AESER
keep that occurrence's severity and seriousness. AESEQ numbers the
subject's events from 1 in visit order (screening, then baseline),
then by visit repeat and form repeat.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
