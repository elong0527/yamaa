Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, ASTDT, AFTIDXFL

The index event for a subject is the subject's first record with AEDECOD
"COVID-19", ordered within the subject by ASTDT, with AESEQ breaking ties
between records that share a start date.

AFTIDXFL is "Y" for a record ordered strictly after the subject's index
event: a record with a later ASTDT than the index event, or the same ASTDT
and a higher AESEQ. AFTIDXFL has no value otherwise: for the index event
itself, for every record of a subject with no index event, and for any
record with no start date.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
