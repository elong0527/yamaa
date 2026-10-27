Following CDISC SDTM standards, use the provided VISITS dataset to create an SV
dataset with one record per subject per visit.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VISITNUM, VISIT, SVSTDTC, SVENDTC, SVUPDES

Use SDTMIG 3.3. DOMAIN is "SV". SVUPDES may have no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/sv.csv.
