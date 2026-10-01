Following CDISC SDTM standards, use the provided AE_RAW and MEDDRA
datasets to create an AE dataset with one record per collected adverse
event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AEDECOD, AEBODSYS

AEDECOD is a preferred term and AEBODSYS a body system, or both hold
NOT CODED. The coded terms follow the recorded dictionary, MedDRA version
26.1.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
