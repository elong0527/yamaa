Following CDISC SDTM standards, use the provided AE_RAW and MEDDRA
datasets to create an AE dataset with one record per collected adverse
event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AELLT, AELLTCD, AEDECOD, AEPTCD,
AEHLT, AEHLTCD, AEHLGT, AEHLGTCD, AEBODSYS, AEBODSCD, AESOC, AESOCCD

AETERM keeps the event text as reported, even when it differs from the
selected lowest-level term. AELLTCD keeps the coder-assigned lowest-level
term code. Every code is written as its digits alone, with no decimal
point. AELLT, AEDECOD and AEPTCD, AEHLT and AEHLTCD, AEHLGT and AEHLGTCD,
and AEBODSYS and AEBODSCD come from the dictionary path for that code.
When the code has a primary and a secondary path, use the primary path for
every hierarchy field; the extract marks the primary path with PRIMARY_SOC
Y. AESOC and AESOCCD repeat the primary system organ class name and code.
An unknown or absent code leaves the hierarchy with no value, while an
unknown assigned code stays visible in AELLTCD.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
