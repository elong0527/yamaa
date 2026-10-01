Following CDISC SDTM standards, use the provided TU_RAW dataset to create
a TU dataset with one record per identified tumor or lesion.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, TUSEQ, TULNKID, TUTESTCD, TUTEST, TUORRES,
TUSTRESC, TULOC, TULAT, TUMETHOD, TUEVAL, VISITNUM, VISIT, TUDTC

Read the source datasets from /app/input and save the completed dataset as
/app/output/tu.csv.
