Following CDISC ADaM standards, use the provided AE dataset to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, ASEV, ASEVN, SEVRANK, SEVLVL

ASEV is the reported severity, and is empty when severity was not
reported. ASEVN is the numeric form of ASEV: 1 for MILD, 2 for
MODERATE, and 3 for SEVERE, and is empty when ASEV is empty. SEVRANK
ranks the subject's events by ASEVN, worst first. Events with equal
severity share a rank and the ranks they would otherwise fill are
skipped, so a rank of 1 means the subject reported nothing worse.
SEVLVL numbers the distinct severities the subject reported, worst
first with each severity counted once. An event with no reported
severity sorts after every reported one, and unreported events share
one rank with each other. A subject with a single event carries rank
1, and when none of a subject's events has a reported severity every
event shares rank 1.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
