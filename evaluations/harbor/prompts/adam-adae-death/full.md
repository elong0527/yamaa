Following CDISC ADaM standards, use the provided AE and DM datasets to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, ASTDT, DTHFL, DTHCAUS, DTHDT

A fatal event is an event whose outcome is recorded as "FATAL". DTHFL is
"Y" on every event of a subject with a fatal event or a death date in DM,
and has no value on every other event. DTHCAUS is the coded term of the
subject's fatal event, and has no value when no event of the subject was
reported fatal. DTHDT is the start date of the subject's fatal event.
When the subject has no fatal event, DTHDT is the death date recorded
for the subject in DM, and a death recorded only in DM gives a date but
no cause. DTHDT has no value when the subject has neither a fatal event
nor a death date in DM. When a subject has more than one fatal event,
the fatal event with the latest start date supplies DTHCAUS and DTHDT,
and a tie on start date goes to the fatal event with the highest AESEQ.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
