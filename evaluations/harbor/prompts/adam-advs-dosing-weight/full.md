Following CDISC ADaM standards, use the provided ADSL and VS datasets to create
an ADVS dataset with one record per subject per treatment cycle.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, AVISITN, ADT, AVAL, ANCHORWT, BSA

ASEQ is the VS sequence number VSSEQ, without renumbering.
AVISITN is the cycle number in VISITNUM; ADT is the collection date in
VSDTC, and AVAL is the collected weight in kilograms in VSSTRESN.

Within each study and subject, ANCHORWT is the weight used for dosing in
kilograms, starting with the first observed weight in ascending cycle order.
At a later cycle with a measured weight at least 10% above or below the
prior retained dosing weight, ANCHORWT is that measured weight, including a
change of exactly 10%.
The new weight applies on the triggering cycle too.
At other cycles, including a missing weight, ANCHORWT is the prior retained
dosing weight.

BSA is the square root of (ANCHORWT times the subject's baseline height in
centimetres from ADSL divided by 3600), in square metres, rounded to two
decimal places.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
