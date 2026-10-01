Following CDISC SDTM standards, use the provided TR_RAW dataset to create
a TR dataset with one record per collected tumor assessment.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, TRSEQ, TRLNKID, TRTESTCD, TRTEST, TRORRES,
TRORRESU, TRSTRESC, TRSTRESN, TRSTRESU, TRSTAT, TRMETHOD, TREVAL, VISITNUM,
TRDTC

TRTESTCD is DIAMETER for a measured lesion or TUMSTATE for a lesion state;
TRTEST is Diameter or Tumor State. TRORRES is a diameter, TOO SMALL TO
MEASURE, a state such as PRESENT, or has no value. TRORRESU is mm or cm for
a diameter, or has no value.

TRSTRESC is the diameter in mm written as text (a whole number has no
decimal point), 5 for a lesion too small to measure, a lesion state, or has
no value. TRSTRESN is the diameter in mm as a number (a value collected in
cm is multiplied by 10), or 5 for a lesion too small to measure. TRSTRESU is
mm or has no value. TRSTAT is NOT DONE or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/tr.csv.
