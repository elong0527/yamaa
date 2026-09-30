Following CDISC ADaM standards, use the provided ADSL and TU datasets
to create an ADRS dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVALC, AVAL

Use PARAMCD "MDIS" and PARAM Measurable Disease at Baseline.

AVALC is Y when the subject has at least one screening tumor
identification record (TUTESTCD of TUMIDENT, VISIT of SCREENING)
whose result is target disease (TUSTRESC of TARGET); otherwise N. A
screening record with a blank result, or a result such as NON-TARGET
or BENIGN ABNORMALITY, is not target disease, and neither are target
records from later visits. AVAL is 1 when AVALC is Y and 0 when AVALC
is N.

Every subject in the subject-level dataset gets one flag record, even
with no screening tumor assessment, so such a subject is N and 0. A
tumor record for a subject outside the subject-level dataset
contributes to no flag and creates none.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
