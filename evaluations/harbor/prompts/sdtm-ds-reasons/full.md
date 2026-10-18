Following CDISC SDTM standards, use the provided ODM dataset to create a DS
dataset with one record per subject for each of the two milestones: end of
treatment and end of study.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, DSSEQ, DSCAT, DSSCAT, DSTERM, DSDECOD, DSSTDTC

- DOMAIN is "DS".
- STUDYID is the ODM study OID and USUBJID is the ODM subject key,
  each as given.
- DSSEQ is 1 for the end-of-treatment milestone and 2 for the
  end-of-study milestone.
- DSCAT is always "DISPOSITION EVENT".
- DSSCAT is "STUDY TREATMENT" for the end-of-treatment milestone and
  "STUDY" for the end-of-study milestone.
- DSTERM is "COMPLETED" when the completion flag for the milestone is
  "COMPLETED", and the collected reason text otherwise.
- DSDECOD is "COMPLETED" when the completion flag for the
  milestone is "COMPLETED", and the collected controlled code
  otherwise.
- The collected controlled code is "ADVERSE EVENT" or "OTHER".
- DSSTDTC is the start date collected for the milestone.
- The completion flag, reason text, controlled code, and start date
  of a milestone are collected as separate items, and the record
  for the milestone carries the values of that milestone's items,
  in whatever order the input rows hold them.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ds.csv.
