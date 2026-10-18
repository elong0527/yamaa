Following CDISC SDTM standards, use the provided TR_RAW dataset to create a TR
dataset with one record per collected tumor assessment.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, TRSEQ, TRLNKID, TRTESTCD, TRTEST, TRORRES, TRORRESU,
TRSTRESC, TRSTRESN, TRSTRESU, TRSTAT, TRMETHOD, TREVAL, VISITNUM, TRDTC

DOMAIN is "TR". STUDYID and USUBJID are as given in TR_RAW, and TRSEQ is
the sequence number as collected. TRLNKID is the link identifier tying
the assessment to its lesion in the tumor identification (TU) domain;
target, non-target, and new lesions each keep their own identifier.
TRTESTCD is the test code as collected: "DIAMETER" for a measured lesion
or "TUMSTATE" for a lesion state. TRTEST is the test name as collected:
"Diameter" or "Tumor State". TRORRES is the result exactly as collected:
a diameter, "TOO SMALL TO MEASURE", a state such as "PRESENT", or no
value when the lesion was not assessed. TRORRESU is the unit exactly as
collected: "mm" or "cm" for a diameter, and no value otherwise.
TRSTRESN is the standardized numeric result in mm: a diameter collected
in mm keeps its collected value, a diameter collected in cm is
multiplied by 10, and a lesion too small to measure takes 5. TRSTRESN
has no value for a lesion state or for a lesion that was not assessed.
TRSTRESC is the standardized character result: for a diameter, the
standardized numeric result in mm written as text, so a lesion too
small to measure gives "5"; a lesion state is kept as collected; and it
has no value when the lesion was not assessed. TRSTRESU is "mm" whenever
TRSTRESN has a value, and has no value otherwise. TRSTAT is "NOT DONE"
for a lesion not assessed at a visit, and has no value otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/tr.csv.
