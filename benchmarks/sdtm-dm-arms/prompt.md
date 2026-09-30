Following CDISC SDTM standards, use the provided RAND and EX datasets
to create a DM dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, ARMCD, ARM, ACTARMCD, ACTARM, ARMNRS, ACTARMUD

ARMCD and ARM are the planned arm code and description from
randomization, and have no value when the subject was never randomized.
ACTARMCD and ACTARM are the actual arm code and description for the
treatment received: PLACEBO gives PBO and Placebo, and VITAMIN D3 gives
TRT and Vitamin D3. They have no value when the subject was never
treated or the treatment matches no planned arm. ARMNRS is the reason
an arm is blank: SCREEN FAILURE when a subject with neither arm is
flagged as a screen failure, NOT ASSIGNED when neither arm applies
without that flag, and NOT TREATED for a randomized subject never
treated. It has no value otherwise, including for a treatment that
matches no planned arm. ACTARMUD keeps the treatment received as
collected, but only when it matches no planned arm, and has no value
otherwise. The planned arm follows randomization while the actual arm
follows exposure, so a subject treated with the other planned arm
carries different codes in each.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
