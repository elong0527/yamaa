Following CDISC ADaM standards, use the provided ADAE_RAW dataset to create an
ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEBODSYS, AEDECOD, ASTDT, AESEV, TRTEMFL, ASEVN,
AOCCPIFL

ASEVN is the numeric rank of AESEV: 1 for "MILD", 2 for "MODERATE", and
3 for "SEVERE". ASEVN has no value when AESEV has no value. An event is
eligible for AOCCPIFL only when it is treatment-emergent (TRTEMFL is "Y")
and has a graded severity (ASEVN has a value). For each subject and
preferred term (AEDECOD), AOCCPIFL is "Y" on the eligible event with the
greatest ASEVN. When eligible events tie on ASEVN, the event with the
earliest ASTDT is flagged; a further tie goes to the event with the
lowest AESEQ, so exactly one event is flagged for each subject and
preferred term that has an eligible event. AOCCPIFL has no value on every
other event.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
