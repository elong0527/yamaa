Following CDISC ADaM standards, use the provided AE and DM datasets to
create an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, ASTDT, DTHFL, DTHCAUS, DTHDT

Derive the subject-level death information from fatal adverse events and
the death date in DM, and show it on every adverse event of the subject:
- DTHFL is Y when the subject has a fatal adverse event or a death date in
  DM, and has no value otherwise.
- DTHCAUS contains the coded term of the fatal adverse event. It has no
  value when the death is recorded only in DM.
- DTHDT contains the date of death: the start date of the fatal adverse
  event when there is one, and the DM death date otherwise.
- When a subject has multiple fatal adverse events, use the most recent
  event. If multiple fatal events start on the same date, use the event
  with the highest AESEQ.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
