Following CDISC ADaM standards, use the provided AE and SUPP datasets to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, AESEV

AEDECOD is the dictionary-derived term collected for the event; it is
always present. AESEV is the severity/intensity recorded for the event.
Severity is matched to an event on STUDYID, USUBJID, and AESEQ together,
so two events for one subject keep their own severities apart. AESEV has
no value when the event has no supplemental record, or when its
supplemental record carries no severity. A supplemental record for an
event that was never collected creates no row.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
