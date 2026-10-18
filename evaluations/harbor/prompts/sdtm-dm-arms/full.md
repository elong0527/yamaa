Following CDISC SDTM standards, use the provided EX and RAND datasets to create
a DM dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, ARMCD, ARM, ACTARMCD, ACTARM, ARMNRS, ACTARMUD

ARMCD and ARM are the planned arm code and description from the
subject's randomization record, and have no value when the subject was
never randomized. ACTARMCD and ACTARM are the code and description of
the planned arm for the treatment the subject received. A treatment of
"PLACEBO" gives ACTARMCD "PBO" and ACTARM "Placebo". ACTARMCD and
ACTARM have no value when the subject was never treated or the
treatment received matches no planned arm. The planned arm follows
randomization and the actual arm follows the treatment received, so a
subject treated with the other planned arm carries different codes in
ARMCD and ACTARMCD. ARMNRS is "SCREEN FAILURE" when the subject has
neither a planned arm nor an actual arm and is flagged as a screen
failure. ARMNRS is "NOT ASSIGNED" when the subject has neither a
planned arm nor an actual arm and is not flagged as a screen failure.
ARMNRS is "NOT TREATED" when the subject was randomized but never
treated. ARMNRS has no value in every other case, including when the
treatment received matches no planned arm. ACTARMUD is the treatment
received as collected when it matches no planned arm, and has no value
otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
