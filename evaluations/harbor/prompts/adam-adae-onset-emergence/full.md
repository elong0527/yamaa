Following CDISC ADaM standards, use the provided ADSL and AE datasets to create
an ADAE dataset with one record per AE record, the events of an untreated
subject included.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AESER, ASTDTM, TRTSDTM, TRTEDTM, EOTSTT,
TEENDDTM, TRTEMFL, AOCCFL

Treatment emergence is decided at the moment, to the second, not the day. A
subject is untreated when TRTSDTM has no value.
- ASTDTM is the collected onset, AESTDTC, as a date and time to the second;
  an onset collected without seconds falls on second zero. ASTDTM has no
  value when AESTDTC is empty.
- TEENDDTM is the end of the treatment-emergent window: 30 days after TRTEDTM
  when AESER is "N", and 90 days after TRTEDTM for any other event, serious
  or with AESER not recorded. A day is 24 hours. TEENDDTM has no value when
  EOTSTT is "ONGOING", and for an untreated subject.
- TRTEMFL is "Y" when ASTDTM is at or after TRTSDTM and at or before
  TEENDDTM. When EOTSTT is "ONGOING", TRTEMFL is "Y" for any ASTDTM at or
  after TRTSDTM. TRTEMFL is also "Y" for an event with no ASTDTM when the
  subject was treated. TRTEMFL has no value otherwise, and for every event of
  an untreated subject.
- AOCCFL is "Y" on the subject's earliest treatment-emergent event by ASTDTM,
  the lower AESEQ settling a tie at the same second. AOCCFL has no value on
  every other record, a treatment-emergent event with no ASTDTM included.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
