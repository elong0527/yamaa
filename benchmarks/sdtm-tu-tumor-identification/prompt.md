Following CDISC SDTM standards, use the provided TU_RAW dataset to create
a TU dataset with one record per identified tumor or lesion.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, TUSEQ, TULNKID, TUTESTCD, TUTEST, TUORRES,
TUSTRESC, TULOC, TULAT, TUMETHOD, TUEVAL, VISITNUM, VISIT, TUDTC

TUSEQ numbers a subject's identification records in collection order,
starting at 1. TULNKID ties the lesion to its assessments in the tumor
results domain: T plus the lesion number for a target lesion, NT plus the
number for a non-target lesion, and NEW plus the number for a new lesion,
the number written in two digits as collected (target lesion 3 is T03).
TUTESTCD is TUMIDENT and TUTEST is Tumor Identification on every record.
TUORRES is the lesion category as collected, standardized to TARGET,
NON-TARGET, or NEW; TUSTRESC repeats the standardized category. TULOC is
the anatomical location as collected, standardized to uppercase. TULAT is
the laterality as collected, standardized to LEFT or RIGHT, and has no
value when the site recorded none. TUMETHOD, TUEVAL, VISITNUM, and TUDTC
are as collected.

A lymph-node target lesion is identified like any other target lesion: it
keeps a T link identifier and its category stays TARGET, while its
location names it as a lymph node. A new lesion is identified once, at the
visit where it first appears.

Read the source datasets from /app/input and save the completed dataset as
/app/output/tu.csv.
