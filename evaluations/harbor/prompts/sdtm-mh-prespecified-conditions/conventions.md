Following CDISC SDTM standards, use the provided MH_ITEMS and ODM datasets
to create an MH dataset with one record per condition asked about on the
checklist or volunteered in free text.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, MHSEQ, MHTERM, MHCAT, MHPRESP, MHOCCUR, MHSTAT

DOMAIN is "MH". MHCAT is "DISEASE-SPECIFIC HISTORY" or "GENERAL
HISTORY". MHPRESP is "Y" or has no value. MHOCCUR is "Y" or "N", or has
no value. MHSTAT is "NOT DONE" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/mh.csv.
