Following CDISC SDTM standards, use the provided AE, CM, TR, and TU datasets to
create a RELREC dataset with one record per linked record or related dataset.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID

RDOMAIN is "TU", "TR", "AE", or "CM". IDVAR is "TULNKID", "TRLNKID",
"AESEQ", or "CMSEQ". IDVARVAL is the sequence number of the related
record, as text, or has no value. RELTYPE is "ONE" or "MANY", or has no
value. The dataset-level rows share RELID "1".

Read the source datasets from /app/input and save the completed dataset as
/app/output/relrec.csv.
