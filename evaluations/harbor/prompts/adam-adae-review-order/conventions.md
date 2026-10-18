Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, AETERM, ASTDT, ASEV

For each subject, the listing is in review order. "SEVERE" events come
before "MODERATE" events, and "MODERATE" events come before "MILD"
events. An event with no reported severity comes after every event with
a reported severity. Within one severity, an event with no onset date
comes first, and the dated events follow with the earliest onset first.
Events alike in severity and onset date are presented in ASEQ order.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
