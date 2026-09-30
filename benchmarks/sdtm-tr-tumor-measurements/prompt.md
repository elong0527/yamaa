Following CDISC SDTM standards, use the provided TR_RAW dataset to create
a TR dataset with one record per collected tumor assessment.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, TRSEQ, TRLNKID, TRTESTCD, TRTEST, TRORRES,
TRORRESU, TRSTRESC, TRSTRESN, TRSTRESU, TRSTAT, TRMETHOD, TREVAL, VISITNUM,
TRDTC

TRLNKID is the link identifier tying the assessment to its lesion in the
tumor identification domain. TRTESTCD is DIAMETER for a measured lesion or
TUMSTATE for a lesion state; TRTEST is Diameter or Tumor State. TRORRES is
the result exactly as collected, never overwritten: a diameter, TOO SMALL
TO MEASURE, a state such as PRESENT, or has no value when the lesion was
not assessed. TRORRESU is the unit exactly as collected (mm or cm for a
diameter), and has no value otherwise.

TRSTRESC is the diameter in mm written as text (a whole number has no
decimal point), 5 for a lesion too small to measure, or a lesion state
kept as collected; it has no value when the lesion was not assessed.
TRSTRESN is the diameter in mm as a number (a value collected in cm is
multiplied by 10), or 5 for a lesion too small to measure; it has no value
for lesion states and for lesions not assessed. TRSTRESU is mm whenever a
standardized numeric result exists, and has no value otherwise. TRSTAT is
NOT DONE for a lesion not assessed at a visit, and has no value otherwise.
TRMETHOD, TREVAL, and VISITNUM are as collected; TRDTC is the assessment
date as collected, and has no value when the lesion was not assessed.

A lymph-node target lesion contributes its short axis as its diameter. A
lesion too small to measure is not a zero: it takes the study convention
of 5 mm. A lesion not assessed stays out of every standardized column
rather than counting as zero.

Read the source datasets from /app/input and save the completed dataset as
/app/output/tr.csv.
