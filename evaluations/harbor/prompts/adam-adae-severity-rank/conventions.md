Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, ASEV, ASEVN, SEVRANK, SEVLVL

ASEVN is 1 for "MILD", 2 for "MODERATE", and 3 for "SEVERE", or has no
value. SEVRANK is the rank of the event among the subject's events
ordered by ASEVN, worst first: events with equal ASEVN share a rank, and
the ranks they would otherwise fill are skipped. An event with no
reported severity ranks after every event with a reported severity, and
events with no reported severity share one rank with each other. SEVLVL
is the dense rank of the event: the subject's worst severity is 1, the
next distinct severity is 2, and each further distinct severity takes
the next number, with an unreported severity counting as one more
distinct severity.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
