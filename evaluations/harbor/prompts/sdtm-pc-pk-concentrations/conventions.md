Following CDISC SDTM standards, use the provided LAB_TRANSFER dataset to create
a PC dataset with one record per laboratory sample.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PCSEQ, PCREFID, PCTESTCD, PCTEST, PCORRES, PCORRESU,
PCSTRESC, PCSTRESN, PCSTRESU, PCSPEC, PCDTC, PCTPT, PCTPTNUM

PCTESTCD is "ZX201", PCTEST is "ZX-201", and PCSPEC is "PLASMA".
PCSTRESC includes "BLQ" and the reported decimal representation.
PCSTRESN is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/pc.csv.
