Following CDISC SDTM standards, use the provided ODM and MH_ITEMS datasets
to create an MH dataset with one record per checklist condition and per
volunteered free-text condition.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, MHSEQ, MHTERM, MHCAT, MHPRESP, MHOCCUR, MHSTAT

Read the source datasets from /app/input and save the completed dataset as
/app/output/mh.csv.
