Following CDISC SDTM standards, use the provided ODM, CODING, and
WHODRUG datasets to create a CM dataset with one record per reported
medication.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT, CMDECOD, CMCLAS, CMCLASCD

Read the source datasets from /app/input and save the completed dataset as
/app/output/cm.csv.
