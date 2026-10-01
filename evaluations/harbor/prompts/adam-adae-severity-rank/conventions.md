Following CDISC ADaM standards, use the provided AE dataset to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, ASEV, ASEVN, SEVRANK, SEVLVL

ASEVN is the numeric form of ASEV: 1 for MILD, 2 for MODERATE, and 3 for
SEVERE. SEVRANK ranks the subject's events by ASEVN, worst first. Events
with equal severity share a rank and the ranks they would otherwise fill
are skipped. SEVLVL numbers the distinct severities the subject reported,
worst first with each severity counted once. An event with no reported
severity sorts after every reported one, and unreported events share one
rank with each other.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
