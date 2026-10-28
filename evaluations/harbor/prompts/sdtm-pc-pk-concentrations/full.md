Following CDISC SDTM standards, use the provided LAB_TRANSFER dataset to create
a PC dataset with one record per sample per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PCSEQ, PCREFID, PCTESTCD, PCTEST, PCORRES, PCORRESU,
PCSTRESC, PCSTRESN, PCSTRESU, PCSPEC, PCDTC, PCTPT, PCTPTNUM

STUDYID and USUBJID use Protocol and Subject exactly as given.
PCSEQ is consecutive from 1 within each study and subject, in increasing actual
collection date and time (PCDTC), then sample identifier (PCREFID).
DOMAIN is "PC"; PCTESTCD is "ZX201" and PCTEST is "ZX-201"; PCSPEC is "PLASMA".
PCORRES and PCSTRESC contain the reported result text exactly, including "BLQ"
for below the limit of quantification.
PCORRESU and PCSTRESU are "ng/mL".
PCSTRESN is the reported numeric concentration without rounding and has no
value for "BLQ"; no numeric replacement based on a quantification limit is
used.
PCTPTNUM is the nominal time in hours associated with PCTPT: 0 for "PRE", the
number before "H" for planned post-dose labels, and no value for "UNSCHED".
These planned hours depend on the nominal label regardless of the actual
collection time.

Read the source datasets from /app/input and save the completed dataset as
/app/output/pc.csv.
