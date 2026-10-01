Following CDISC ADaM standards, use the provided CE dataset to create
an ADCE dataset with one record per clinical event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CESEQ, CETERM, ASTDT, ASEV, ASEVN, ATOXGRN, AOCCFL

ASEV is the collected severity, and has no value when none was
collected. ASEVN is the numeric rank of ASEV: 1 for MILD through 3 for
SEVERE, and has no value when none was collected. ATOXGRN is the
toxicity grade, equal to ASEVN, so a moderate event reads grade 2; it
has no value when none was collected. AOCCFL is Y on the graded event
with the greatest ATOXGRN for the subject, and has no value otherwise.
Ties break by earliest ASTDT, then lowest CESEQ, so at most one event
per subject is flagged. An event without a grade can never be flagged,
so a subject with no graded event has no flagged event.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adce.csv.
