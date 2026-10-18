Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AESER, ASTDT, SERSEQ

SERSEQ is the running number of a subject's records, ordered by ASTDT:
a subject's earliest record is 1, and each later record takes the next
number. Records that share one ASTDT follow the order of AESEQ. A record
with no ASTDT is numbered after every record of that subject that has
one, and records with no ASTDT follow the order of AESEQ among
themselves.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
