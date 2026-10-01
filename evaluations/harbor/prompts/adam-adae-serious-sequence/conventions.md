Following CDISC ADaM standards, use the provided AE dataset to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AESER, ASTDT, SERSEQ

SERSEQ numbers the subject's serious events in onset order, so the
earliest serious event carries 1. Events sharing one onset date follow
collection sequence order, and a serious event with no onset date is
numbered last. Numbering restarts for each subject.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
