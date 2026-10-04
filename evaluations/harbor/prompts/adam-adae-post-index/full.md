Following CDISC ADaM standards, use the provided AE dataset to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, ASTDT, AFTIDXFL

ASTDT is the event start date. AFTIDXFL is Y for an event ordered
strictly after the subject's index event -- the subject's first
occurrence of the designated index term (COVID-19 in this dataset):
a later start date, or the same start date with a higher sequence
number. It has no value on the index event itself, on every record
of a subject with no index event, and on any record with no start
date. Ordering is within each subject by start date, with AESEQ
breaking ties on the same date.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
