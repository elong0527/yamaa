Following CDISC SDTM standards, use the provided LAB_TRANSFER dataset to create
a PC dataset with one record per sample.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PCSEQ, PCREFID, PCTESTCD, PCTEST, PCORRES, PCORRESU,
PCSTRESC, PCSTRESN, PCSTRESU, PCSPEC, PCDTC, PCTPT, PCTPTNUM

DOMAIN is "PC". STUDYID and USUBJID are the Protocol and Subject identifiers as
supplied, respectively. PCSEQ numbers samples from 1 within each study and
subject by ascending actual collection datetime, then ascending Sample
identifier (PCREFID). PCTESTCD is "ZX201", PCTEST is "ZX-201", and PCSPEC is
"PLASMA". PCORRES and PCSTRESC hold the reported concentration text unchanged,
including "BLQ" and the numeric text with its supplied decimal places. PCORRESU
and PCSTRESU both hold the supplied unit independently of the reported result,
including for "BLQ", with no unit conversion. PCSTRESN holds the numeric
reported concentration without rounding and has no value for "BLQ"; no
limit-based numeric replacement is made. PCTPTNUM is the planned time in hours
represented by the vendor's nominal label in PCTPT: 0 for "PRE", the numeric
value before "H" for planned post-dose labels, and no value for "UNSCHED",
regardless of the actual collection time.

Read the source datasets from /app/input and save the completed dataset as
/app/output/pc.csv.
