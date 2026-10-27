Following CDISC SDTM standards, use the provided MORPHOLOGY dataset to create
an MO dataset with one record per cell type finding per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, MOSEQ, MOTESTCD, MOTEST, MOORRES, MOSTRESC, MODTC

Use SDTMIG 3.3. DOMAIN is "MO". MOTESTCD is "CELLTYPE" and MOTEST is
"Cell Type".

Read the source datasets from /app/input and save the completed dataset as
/app/output/mo.csv.
