Following CDISC ADaM standards, use the provided AE dataset to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, AETERM, ASTDT, ASEV

ASEQ is the collected sequence number. AETERM is the adverse event
term exactly as reported. ASTDT is the collected onset date, and has no
value when the event has no recorded onset date. ASEV is the reported
severity, and has no value when no severity was reported.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
