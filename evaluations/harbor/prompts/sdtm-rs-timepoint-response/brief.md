Following CDISC SDTM standards, use the provided ODM dataset to create an
RS dataset with one record per subject and scheduled tumor assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, AVISITN, ADT, RSSEQ, RSTESTCD, RSTEST, RSSTRESC,
RSSTAT

Read the source datasets from /app/input and save the completed dataset as
/app/output/rs.csv.
