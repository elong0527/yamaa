Following CDISC ADaM standards, use the provided AE dataset to create
an ADAE dataset with one record per serious adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, AESER

AEDECOD is the dictionary term collected for the event. AESER is Y on
every row, because only serious events are kept. A non-serious event
leaves no row.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
