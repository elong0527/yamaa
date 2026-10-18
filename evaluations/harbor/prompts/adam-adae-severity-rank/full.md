Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, ASEV, ASEVN, SEVRANK, SEVLVL

ASEVN is the numeric form of ASEV: 1 for "MILD", 2 for "MODERATE", and 3
for "SEVERE". ASEVN has no value when ASEV has no value. SEVRANK is the
rank of the event among the subject's events ordered by ASEVN, worst
first: events with equal ASEVN share a rank, and the ranks they would
otherwise fill are skipped, so a rank of 1 means the subject reported
nothing worse. An event with no reported severity ranks after every
event with a reported severity, and events with no reported severity
share one rank with each other. SEVLVL is the dense rank of the event:
the subject's worst severity is 1, the next distinct severity is 2, and
each further distinct severity takes the next number, with an
unreported severity counting as one more distinct severity, so the
largest SEVLVL is how many different severities the subject has. A
subject with a single event carries SEVRANK 1 and SEVLVL 1, and when
none of a subject's events has a reported severity, every event carries
SEVRANK 1 and SEVLVL 1.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
