Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per serious adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, AESER

Keep only the events recorded as serious, those with AESER "Y". A
non-serious event, or an event whose seriousness was never recorded (AESER
has no value), leaves no row. AEDECOD is the dictionary term collected for
the event. AESER is "Y" on every row, because only serious events are kept.
Rows follow the order in which the events were collected, so the serious
events of one subject stay interleaved with the serious events of other
subjects exactly as collected.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
