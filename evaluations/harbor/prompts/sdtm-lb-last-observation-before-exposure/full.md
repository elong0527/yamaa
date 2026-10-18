Following CDISC SDTM standards, use the provided DM, LB_MAPPING, and ODM
datasets to create an LB dataset with one record per laboratory result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSPEC, LBORRES, LBDTC, LBSTAT,
LBLOBXFL

DOMAIN is "LB". STUDYID is the study OID in ODM, as given, and USUBJID
is the subject key in ODM, as given. LBSEQ is the item group repeat
number of the result, as collected. LBTESTCD is the test code in
LB_MAPPING for the result item, and LBSPEC is the specimen in
LB_MAPPING for that item. LBTESTCD and LBSPEC together define the
series within which the flag is assigned, and a test measured in more
than one specimen is flagged separately for each specimen. LBORRES is
the result as collected, and has no value on a record that was not
done. LBDTC is the collection date as collected. LBSTAT is the
completion status as collected, "NOT DONE" on a record without a
result. LBLOBXFL is "Y" on the latest record with a result in the
series whose collection date falls on or before the subject's
reference start date in DM, and has no value on every other record. A
record without a result never carries the flag, even when it carries
the latest date. A subject with no reference start date has the flag
on the latest record with a result, since no record falls after
exposure. Dates compare at day precision, so a record collected on the
day of the reference start date still qualifies. Of two results
collected on the same date, the one with the higher LBSEQ carries the
flag.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
