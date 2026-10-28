Following CDISC SDTM standards, use the provided LAB_TRANSFER dataset to create
a PC dataset with one record per sample per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PCSEQ, PCREFID, PCTESTCD, PCTEST, PCORRES, PCORRESU,
PCSTRESC, PCSTRESN, PCSTRESU, PCSPEC, PCDTC, PCTPT, PCTPTNUM

DOMAIN is "PC"; PCTESTCD is "ZX201" and PCTEST is "ZX-201"; PCSPEC is "PLASMA".
PCORRES and PCSTRESC include "BLQ" for below the limit of quantification.
PCSTRESN is not rounded. PCSTRESN and PCTPTNUM may have no value.
PCTPT uses "PRE", planned post-dose labels with "H", and "UNSCHED".

Read the source datasets from /app/input and save the completed dataset as
/app/output/pc.csv.
