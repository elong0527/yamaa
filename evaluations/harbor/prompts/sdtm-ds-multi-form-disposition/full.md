Following CDISC SDTM standards, use the provided ODM dataset to create a DS
dataset with one record per subject per study event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, DSSEQ, DSTERM, DSDECOD, DSCAT, DSSCAT, DSSTDTC

DOMAIN is "DS". STUDYID is the ODM study OID, and USUBJID is the study OID
and the subject key joined with a hyphen. Each collected form gives one
record, and a study event with no form gives none. The study event names
the event: "CONSENT" is informed consent, "RAND" is randomization, "EOT"
is end of treatment and "EOS" is end of study. DSCAT is "PROTOCOL
MILESTONE" for informed consent and randomization and "DISPOSITION EVENT"
for end of treatment and end of study. DSSCAT is "INFORMED CONSENT" for
informed consent, "RANDOMIZATION" for randomization, "END OF TREATMENT"
for end of treatment and "END OF STUDY" for end of study. For a protocol
milestone, DSTERM and DSDECOD both hold the controlled term, "INFORMED
CONSENT OBTAINED" for informed consent and "RANDOMIZED" for randomization.
For end of treatment and end of study, DSTERM is "COMPLETED" when the
form's completion status is "COMPLETED", and at end of study it is "SCREEN
FAILURE" when the completion status is "SCREEN FAILURE"; otherwise DSTERM
is the reason reported on the form. DSDECOD holds the same term as DSTERM
for a milestone, a completion or a screen failure, and otherwise the
standardized reason collected on the form. DSSTDTC is the date collected
on the form. DSSEQ numbers each subject's records from 1 in the order they
happened, by DSSTDTC and then by DSTERM, so records on the same day keep a
fixed order.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ds.csv.
