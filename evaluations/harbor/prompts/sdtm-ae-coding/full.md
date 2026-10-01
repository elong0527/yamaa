Following CDISC SDTM standards, use the provided AE_RAW and MEDDRA
datasets to create an AE dataset with one record per collected adverse
event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AEDECOD, AEBODSYS

AEDECOD is the preferred term of the dictionary entry whose
lowest-level term equals the reported term exactly, including letter
case. AEBODSYS is the body system of that same entry. The preferred
term comes from the matched entry, so it can differ from the reported
term. A reported term with no exact match, or a blank term, gives NOT
CODED in both AEDECOD and AEBODSYS. NOT CODED is not a dictionary
term: it marks an event whose coding must be resolved before delivery.
The coded terms follow the recorded dictionary, MedDRA version 26.1.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
