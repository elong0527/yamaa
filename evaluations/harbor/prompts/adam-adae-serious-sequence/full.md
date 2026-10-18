Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AESER, ASTDT, SERSEQ

SERSEQ is the running number of a subject's serious events, the records
with AESER equal to "Y", ordered by ASTDT: a subject's earliest serious
event is 1, and each later serious event takes the next number. Serious
events that share one ASTDT follow the order of AESEQ. A serious event
with no ASTDT is numbered after every serious event of that subject that
has one, and serious events with no ASTDT follow the order of AESEQ among
themselves. SERSEQ has no value for a record that is not serious, and a
subject with no serious event has no numbered record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
