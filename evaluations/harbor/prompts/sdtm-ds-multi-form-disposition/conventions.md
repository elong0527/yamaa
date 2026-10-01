Following CDISC SDTM standards, use the provided ODM dataset to create
a DS dataset with one record per subject per study event: informed
consent, randomization, end of treatment, and end of study.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, DSSEQ, DSTERM, DSDECOD, DSCAT, DSSCAT, DSSTDTC

Consent gives DSTERM and DSDECOD INFORMED CONSENT OBTAINED; randomization
gives RANDOMIZED. Both are protocol milestones, with DSSCAT INFORMED CONSENT
and RANDOMIZATION. End of treatment gives COMPLETED or the collected reason
with the collected standardized reason; DSSCAT is END OF TREATMENT. End of
study gives COMPLETED, SCREEN FAILURE, or the collected reason with the
collected standardized reason; DSSCAT is END OF STUDY. End-of-treatment and
end-of-study records are disposition events.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ds.csv.
