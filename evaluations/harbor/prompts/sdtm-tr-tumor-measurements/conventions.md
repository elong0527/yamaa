Following CDISC SDTM standards, use the provided TR_RAW dataset to create
a TR dataset with one record per collected tumor assessment.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, TRSEQ, TRLNKID, TRTESTCD, TRTEST, TRORRES,
TRORRESU, TRSTRESC, TRSTRESN, TRSTRESU, TRSTAT, TRMETHOD, TREVAL, VISITNUM,
TRDTC

TRTESTCD is DIAMETER for a measured lesion or TUMSTATE for a lesion state;
TRTEST is Diameter or Tumor State. TRORRES is a diameter, TOO SMALL TO
MEASURE, a state such as PRESENT, or has no value.

In TRSTRESC a whole number has no decimal point. TRSTRESC and TRSTRESN are 5
for a lesion too small to measure. TRSTAT is NOT DONE or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/tr.csv.
