Following CDISC SDTM standards, use the provided LAB_TRANSFER dataset to create
a PC dataset with one record per sample.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PCSEQ, PCREFID, PCTESTCD, PCTEST, PCORRES, PCORRESU,
PCSTRESC, PCSTRESN, PCSTRESU, PCSPEC, PCDTC, PCTPT, PCTPTNUM

DOMAIN is "PC". PCTESTCD is "ZX201", PCTEST is "ZX-201", and PCSPEC is
"PLASMA". PCORRES and PCSTRESC include "BLQ" and numeric text with its supplied
decimal places. PCSTRESN is given without rounding. PCTPT includes "PRE",
"UNSCHED", and planned post-dose labels with the numeric value before "H".

Read the source datasets from /app/input and save the completed dataset as
/app/output/pc.csv.
