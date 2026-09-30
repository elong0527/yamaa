Following CDISC ADaM standards, use the provided AE dataset to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, ASEV, ASEVN

ASEV is the analysis severity in upper case: MILD, MODERATE, SEVERE, or
LIFE-THREATENING. An approved correction reassigns the event of subject
CATH-01-001 with AESEQ 2 to SEVERE. An event with no reported severity
and no applicable correction has no ASEV value. ASEVN is the numeric
rank of ASEV: 1 for MILD through 4 for LIFE-THREATENING. An event with
no ASEV value has no rank. ASEVN reflects the corrected ASEV.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
