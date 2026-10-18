Following CDISC ADaM standards, use the provided ADSL and AE datasets to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, ASTDTM, TRTSDTM, TEENDDTM, TRTEMFL, AOCCFL

TEENDDTM is the end of the treatment-emergent window: 30 days after last
exposure. TRTEMFL is "Y" when ASTDTM is at or after TRTSDTM and at or
before TEENDDTM. TRTEMFL has no value when ASTDTM is before TRTSDTM or
after TEENDDTM. When ASTDTM has no value and TRTSDTM has a value, TRTEMFL
is "Y". When TRTSDTM has no value, TRTEMFL has no value on every record.
Emergence is decided at the moment, not the day. An onset collected
without seconds has its seconds taken as zero, and onsets are compared
at second precision. AOCCFL is "Y" on the subject's earliest
treatment-emergent event by onset moment. When treatment-emergent events
of one subject share the same onset second, the event with the lower
AESEQ is the earliest. AOCCFL has no value on a treatment-emergent event
whose ASTDTM has no value. AOCCFL has no value on every other record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
