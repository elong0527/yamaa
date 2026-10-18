Following CDISC SDTM standards, use the provided TU_RAW dataset to create a TU
dataset with one record per identified tumor or lesion.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, TUSEQ, TULNKID, TUTESTCD, TUTEST, TUORRES, TUSTRESC,
TULOC, TULAT, TUMETHOD, TUEVAL, VISITNUM, VISIT, TUDTC

DOMAIN is "TU" on every record. TULNKID is "T" plus the lesion number
for a target lesion, "NT" plus the lesion number for a non-target
lesion, and "NEW" plus the lesion number for a new lesion. TUTESTCD
is "TUMIDENT" and TUTEST is "Tumor Identification" on every record.
TUORRES is "TARGET", "NON-TARGET", or "NEW". TULOC is the anatomical
location as collected, standardized to uppercase. TULAT is "LEFT" or
"RIGHT", or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/tu.csv.
