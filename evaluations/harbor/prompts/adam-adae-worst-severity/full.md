Following CDISC ADaM standards, use the provided ADAE_RAW dataset to
create an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEBODSYS, AEDECOD, ASTDT, AESEV, TRTEMFL,
AESEVN, AWSEVFL

AESEVN is the numeric rank of AESEV: 1 for MILD through 3 for SEVERE,
and has no value when no severity was collected. AWSEVFL is Y on the
eligible event with the greatest AESEVN for the subject and preferred
term. Only a treatment-emergent event with a graded severity is
eligible, so a preferred term whose events are all ineligible has no
flagged event. Ties break by earliest ASTDT, then lowest AESEQ, so
exactly one event per term is flagged.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
