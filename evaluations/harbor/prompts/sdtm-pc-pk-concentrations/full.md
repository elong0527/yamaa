Following CDISC SDTM standards, use the provided LAB_TRANSFER dataset to create
a PC dataset with one record per laboratory sample.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PCSEQ, PCREFID, PCTESTCD, PCTEST, PCORRES, PCORRESU,
PCSTRESC, PCSTRESN, PCSTRESU, PCSPEC, PCDTC, PCTPT, PCTPTNUM

STUDYID and USUBJID are Protocol and Subject as given.
PCSEQ starts at 1 within each study and subject, in increasing actual
collection time, then increasing sample identifier.
PCTESTCD is "ZX201", PCTEST is "ZX-201", and PCSPEC is "PLASMA".
PCSTRESC equals PCORRES exactly, including "BLQ" and the reported decimal
representation.
PCSTRESN is the numeric reported result, is not rounded, and has no value for
"BLQ"; no limit-based numeric replacement applies.
PCSTRESU equals PCORRESU, including for "BLQ"; both unit columns are
independent of the result, and no unit conversion applies.
PCTPTNUM is 0 for "PRE", the numeric hours before "H" for planned post-dose
labels, and has no value for "UNSCHED".
The nominal label alone determines planned hours, regardless of the actual
collection time.

Read the source datasets from /app/input and save the completed dataset as
/app/output/pc.csv.
