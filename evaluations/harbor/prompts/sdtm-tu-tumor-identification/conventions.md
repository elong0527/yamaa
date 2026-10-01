Following CDISC SDTM standards, use the provided TU_RAW dataset to create
a TU dataset with one record per identified tumor or lesion.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, TUSEQ, TULNKID, TUTESTCD, TUTEST, TUORRES,
TUSTRESC, TULOC, TULAT, TUMETHOD, TUEVAL, VISITNUM, VISIT, TUDTC

TULNKID is T plus the lesion number for a target lesion, NT plus the number
for a non-target lesion, and NEW plus the number for a new lesion, the
number written in two digits as collected (target lesion 3 is T03). TUTESTCD
is TUMIDENT and TUTEST is Tumor Identification on every record. TUORRES is
standardized to TARGET, NON-TARGET, or NEW. TULOC is standardized to
uppercase. TULAT is standardized to LEFT or RIGHT, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/tu.csv.
